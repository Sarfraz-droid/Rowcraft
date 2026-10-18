import type { ReactNode } from "react";
import { ContextMenu, DropdownMenu } from "radix-ui";
import { Icon, type IconName } from "./Icon";

export interface ConnectionAction {
  label: string;
  icon: IconName;
  run(): void;
  disabled?: boolean;
  destructive?: boolean;
  separator?: boolean;
  children?: ConnectionAction[];
}

export function ConnectionMenuItems({ actions, context = false }: { actions: ConnectionAction[]; context?: boolean }) {
  const { Item, Separator, Sub, SubTrigger, Portal, SubContent } = context ? ContextMenu : DropdownMenu;
  return actions.map(action => <div key={action.label} role="presentation">
    {action.separator && <Separator className="connection-menu-separator" />}
    {action.children ? <Sub><SubTrigger className="connection-menu-item" disabled={action.disabled}>
      <Icon name={action.icon} size={14} /><span>{action.label}</span><Icon name="chevronRight" size={12} className="tw:ml-auto" />
    </SubTrigger><Portal><SubContent className="connection-menu" sideOffset={4} collisionPadding={8} aria-label={action.label}><ConnectionMenuItems actions={action.children} context={context} /></SubContent></Portal></Sub> : <Item className={`connection-menu-item${action.destructive ? " destructive" : ""}`} disabled={action.disabled} onSelect={action.run}>
      <Icon name={action.icon} size={14} /><span>{action.label}</span>
    </Item>}
  </div>);
}

export function ConnectionMenu({ name, actions, children }: { name: string; actions: ConnectionAction[]; children: ReactNode }) {
  return <ContextMenu.Root>
    <ContextMenu.Trigger asChild><div className="source-row">
      {children}
      <div className="source-actions"><DropdownMenu.Root><DropdownMenu.Trigger asChild><button type="button" className="icon-button" aria-label={`Actions for ${name}`}><Icon name="more" /></button></DropdownMenu.Trigger>
        <DropdownMenu.Portal><DropdownMenu.Content className="connection-menu" align="start" side="right" sideOffset={5} collisionPadding={8} aria-label={`Actions for ${name}`}>
          <DropdownMenu.Label className="connection-menu-label">{name}</DropdownMenu.Label>
          <ConnectionMenuItems actions={actions} />
        </DropdownMenu.Content></DropdownMenu.Portal>
      </DropdownMenu.Root></div>
    </div></ContextMenu.Trigger>
    <ContextMenu.Portal><ContextMenu.Content className="connection-menu" collisionPadding={8} aria-label={`Actions for ${name}`}>
      <ContextMenu.Label className="connection-menu-label">{name}</ContextMenu.Label>
      <ConnectionMenuItems actions={actions} context />
    </ContextMenu.Content></ContextMenu.Portal>
  </ContextMenu.Root>;
}
