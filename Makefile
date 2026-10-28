# Rowcraft developer commands. Run `make help`.

CARGO      ?= cargo
# Low disk use: incremental artifacts roughly double target/. Override with `CARGO_INCREMENTAL=1`.
export CARGO_INCREMENTAL ?= 0
ROWCRAFT         := target/debug/rowcraft

# Desktop app (Tauri + React). Local binaries are called directly: `pnpm run` first checks
# dependency status, which fails on some setups.
DESKTOP    := apps/rowcraft-desktop
# Private key that signs in-app update archives (docs/installers.md); never commit it.
UPDATER_KEY ?= $(HOME)/.tauri/rowcraft-updater.key
DESKTOP_CFG := $(CURDIR)/.dev/desktop-cfg
DESKTOP_DB  := $(CURDIR)/.dev/desktop-demo.sqlite

# Standalone landing page (no dependency installation or build required).
LANDING      := $(CURDIR)/apps/landing
LANDING_PORT ?= 4321
PYTHON       ?= python3

# Throwaway PostgreSQL for live tests and the demo. Everything lives in .dev/ (git-ignored).
PG_BIN     ?= $(dir $(shell command -v pg_ctl 2>/dev/null || echo /opt/homebrew/opt/postgresql@14/bin/pg_ctl))
PG_DIR     := $(CURDIR)/.dev/pgdata
PG_PORT    ?= 54329
PG_PASS    ?= devpw
DEV_CFG    := $(CURDIR)/.dev/cfg

# Throwaway OpenSSH server (no root needed) for SSH tunnel tests.
SSH_DIR    := $(CURDIR)/.dev/ssh
SSH_PORT   ?= 2222

.DEFAULT_GOAL := help
.PHONY: help build cli gui gui-install installer release release-finish run-cli run-gui web test-desktop desktop-demo desktop-typecheck dialects test lint fmt check \
        pg-up pg-down test-live demo clean landing landing-check

help: ## Show this help
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z_-]+:.*## / {printf "  \033[1m%-12s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

build: ## Build everything except the GUI
	$(CARGO) build --workspace --exclude rowcraft-desktop

cli: ## Build the `rowcraft` CLI (target/debug/rowcraft)
	$(CARGO) build -p rowcraft-cli

gui-install: ## Install frontend dependencies (Node 22.12+ and pnpm 11)
	pnpm --dir $(DESKTOP) install --frozen-lockfile

gui: ## Build the desktop app without bundling (run gui-install once first)
	cd $(DESKTOP) && node_modules/.bin/tauri build --no-bundle

installer: ## Build installers for the current OS (run gui-install once first)
	@# Update archives need the updater signing key; without it, build installers only.
	cd $(DESKTOP) && if [ -n "$$TAURI_SIGNING_PRIVATE_KEY" ] || [ -f "$(UPDATER_KEY)" ]; then \
		TAURI_SIGNING_PRIVATE_KEY="$${TAURI_SIGNING_PRIVATE_KEY:-$(UPDATER_KEY)}" node_modules/.bin/tauri build; \
	else \
		node_modules/.bin/tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}'; \
	fi

release: ## Release VERSION=x.y.z: bump, push main + tag, wait for installers, publish as latest
	@test -n "$(VERSION)" || { echo "usage: make release VERSION=x.y.z"; exit 1; }
	scripts/release.sh $(VERSION)

release-finish: ## Resume a release whose tag is pushed: wait, publish, verify (VERSION=x.y.z)
	@test -n "$(VERSION)" || { echo "usage: make release-finish VERSION=x.y.z"; exit 1; }
	scripts/release.sh $(VERSION) --finish

run-gui: ## Launch the desktop app with frontend hot reload (uses your real config)
	cd $(DESKTOP) && node_modules/.bin/tauri dev

desktop-demo: ## Launch the desktop app on a throwaway SQLite database and config (.dev/)
	@mkdir -p .dev
	@rm -f $(DESKTOP_DB)
	@command -v sqlite3 >/dev/null || { echo "sqlite3 is needed to create the demo database"; exit 1; }
	@sqlite3 $(DESKTOP_DB) "create table people(id integer primary key, name text not null); insert into people(name) values ('ann'),('bob'),('cy'); create view people_view as select * from people;"
	@rm -rf $(DESKTOP_CFG) && mkdir -p $(DESKTOP_CFG)
	@printf '[[datasource]]\nname = "demo"\ndialect = "sqlite"\ndatabase = "%s"\n' "$(DESKTOP_DB)" > $(DESKTOP_CFG)/datasources.toml
	@echo "Demo data source 'demo' -> $(DESKTOP_DB)"
	cd $(DESKTOP) && ROWCRAFT_CONFIG_DIR=$(DESKTOP_CFG) node_modules/.bin/tauri dev

web: ## Design the UI in a browser with a mock backend (http://localhost:1420)
	cd $(DESKTOP) && node_modules/.bin/vite

landing: ## Serve the landing page at http://127.0.0.1:4321 (override LANDING_PORT)
	@echo "Landing page: http://127.0.0.1:$(LANDING_PORT) (Ctrl+C to stop)"
	$(PYTHON) -m http.server $(LANDING_PORT) --bind 127.0.0.1 --directory "$(LANDING)"

landing-check: ## Check landing page JavaScript and run sample-query tests (requires Node/npm)
	npm --prefix "$(LANDING)" run check
	npm --prefix "$(LANDING)" test

desktop-typecheck: ## Type-check the frontend
	cd $(DESKTOP) && node_modules/.bin/tsc --noEmit

test-desktop: desktop-typecheck ## Test the desktop service and frontend without opening a window
	$(CARGO) test -p rowcraft-ui-model
	cd $(DESKTOP) && node_modules/.bin/vitest run
	$(CARGO) check -p rowcraft-desktop

dialects: cli ## List the databases this build supports
	$(ROWCRAFT) dialects

test: ## Unit tests (live PostgreSQL tests are skipped)
	$(CARGO) test --workspace --exclude rowcraft-desktop

lint: ## Clippy, warnings as errors
	$(CARGO) clippy --workspace --exclude rowcraft-desktop --all-targets -- -D warnings

fmt: ## Format all code
	$(CARGO) fmt --all

check: fmt lint test ## Format, lint, test

pg-up: ## Start a throwaway PostgreSQL on 127.0.0.1:$(PG_PORT)
	@if [ -f $(PG_DIR)/postmaster.pid ] && $(PG_BIN)pg_ctl -D $(PG_DIR) status >/dev/null 2>&1; then \
	  echo "postgres already running on 127.0.0.1:$(PG_PORT)"; \
	else \
	  mkdir -p .dev; \
	  if [ ! -d $(PG_DIR) ]; then \
	    echo '$(PG_PASS)' > .dev/pwfile && \
	    $(PG_BIN)initdb -D $(PG_DIR) -U postgres --auth=scram-sha-256 --pwfile=.dev/pwfile >/dev/null && \
	    rm .dev/pwfile; \
	  fi; \
	  $(PG_BIN)pg_ctl -D $(PG_DIR) -o "-p $(PG_PORT) -c unix_socket_directories=''" \
	    -l .dev/pg.log -w start >/dev/null && echo "postgres up on 127.0.0.1:$(PG_PORT) (user postgres, password $(PG_PASS))"; \
	fi

pg-down: ## Stop the throwaway PostgreSQL
	@if [ -f $(PG_DIR)/postmaster.pid ]; then $(PG_BIN)pg_ctl -D $(PG_DIR) -m fast stop >/dev/null && echo "postgres stopped"; else echo "not running"; fi

ssh-up: ## Start a throwaway OpenSSH server on 127.0.0.1:$(SSH_PORT) with test keys
	@if [ -f $(SSH_DIR)/sshd.pid ] && kill -0 $$(cat $(SSH_DIR)/sshd.pid) 2>/dev/null; then \
	  echo "sshd already running on 127.0.0.1:$(SSH_PORT)"; \
	else \
	  rm -rf $(SSH_DIR) && mkdir -p $(SSH_DIR) && \
	  ssh-keygen -q -t ed25519 -N '' -C rowcraft-host -f $(SSH_DIR)/host_ed25519 && \
	  ssh-keygen -q -t ed25519 -N '' -C rowcraft-client -f $(SSH_DIR)/client && \
	  ssh-keygen -q -t ed25519 -N 's3cret' -C rowcraft-client-enc -f $(SSH_DIR)/client_enc && \
	  cat $(SSH_DIR)/client.pub $(SSH_DIR)/client_enc.pub > $(SSH_DIR)/authorized_keys && \
	  printf '[127.0.0.1]:$(SSH_PORT) %s\n' "$$(cut -d' ' -f1,2 $(SSH_DIR)/host_ed25519.pub)" > $(SSH_DIR)/known_hosts && \
	  printf 'Port $(SSH_PORT)\nListenAddress 127.0.0.1\nHostKey $(SSH_DIR)/host_ed25519\nPidFile $(SSH_DIR)/sshd.pid\nAuthorizedKeysFile $(SSH_DIR)/authorized_keys\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nPubkeyAuthentication yes\nUsePAM no\nAllowTcpForwarding local\nStrictModes no\nLogLevel ERROR\n' > $(SSH_DIR)/sshd_config && \
	  /usr/sbin/sshd -f $(SSH_DIR)/sshd_config -E $(SSH_DIR)/sshd.log && sleep 1 && \
	  echo "sshd up on 127.0.0.1:$(SSH_PORT) (keys in $(SSH_DIR))"; \
	fi

ssh-down: ## Stop the throwaway OpenSSH server
	@if [ -f $(SSH_DIR)/sshd.pid ]; then kill $$(cat $(SSH_DIR)/sshd.pid) 2>/dev/null && echo "sshd stopped"; else echo "not running"; fi

test-ssh: ssh-up ## Run the SSH tunnel tests, including the ones against a real OpenSSH server
	ROWCRAFT_TEST_SSH_PORT=$(SSH_PORT) ROWCRAFT_TEST_SSH_USER=$$(whoami) ROWCRAFT_TEST_SSH_DIR=$(SSH_DIR) $(CARGO) test -p rowcraft-ssh

test-live: pg-up ## Run all tests including the live PostgreSQL ones
	ROWCRAFT_TEST_PG_PORT=$(PG_PORT) ROWCRAFT_TEST_PG_USER=postgres ROWCRAFT_TEST_PG_PASSWORD=$(PG_PASS) \
	  $(CARGO) test --workspace --exclude rowcraft-desktop

demo: cli pg-up ## Create a sample schema and introspect it with `rowcraft`
	@PGPASSWORD=$(PG_PASS) $(PG_BIN)psql -h 127.0.0.1 -p $(PG_PORT) -U postgres -q \
	  -c "drop schema if exists shop cascade" -c "create schema shop" \
	  -c "create table shop.customers (id serial primary key, email text not null unique)" \
	  -c "create table shop.orders (id bigint generated by default as identity primary key, customer_id int references shop.customers(id) on delete cascade, total numeric(10,2) check (total >= 0))" \
	  -c "create view shop.big as select * from shop.orders where total > 100" 2>/dev/null
	@export ROWCRAFT_CONFIG_DIR=$(DEV_CFG) DEMO_PW=$(PG_PASS); \
	  $(ROWCRAFT) ds rm demo >/dev/null 2>&1; \
	  $(ROWCRAFT) ds add demo --host 127.0.0.1 --port $(PG_PORT) --database postgres --user postgres \
	    --password-env DEMO_PW --ssl-mode disable && \
	  echo && $(ROWCRAFT) introspect -d demo -s shop && \
	  echo "--- query" && $(ROWCRAFT) query -d demo "select table_name from information_schema.tables where table_schema = 'shop' order by 1"
	@echo; echo "Try it yourself:"; \
	  echo "  export ROWCRAFT_CONFIG_DIR=$(DEV_CFG) DEMO_PW=$(PG_PASS)"; \
	  echo "  $(ROWCRAFT) introspect -d demo -s shop -f json"

clean: pg-down ssh-down ## Stop PostgreSQL and delete .dev/ and target/
	rm -rf .dev
	$(CARGO) clean
