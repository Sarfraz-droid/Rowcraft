import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";

import type { QueryOutput } from "../api";
import { GUTTER_WIDTH, ROW_HEIGHT, cellLabel, columnWidth } from "../grid";
import { Icon } from "./Icon";
import { SelectField } from "./SelectField";

const pageSizes = [25, 50, 100, 250].map(size => ({ value: String(size), label: String(size) }));

/** Only the rows in view are in the DOM, so a large result scrolls as smoothly as a small one. */
export function ResultGrid({ result }: { result: QueryOutput | null }) {
  const scroller = useRef<HTMLDivElement>(null);
  const rows = result?.rows ?? [];
  const [pageSize, setPageSize] = useState(25);
  const [pagination, setPagination] = useState({ result, page: 0 });
  const pageCount = Math.max(1, Math.ceil(rows.length / pageSize));
  const page = pagination.result === result ? Math.min(pagination.page, pageCount - 1) : 0;
  const start = page * pageSize;
  const end = Math.min(start + pageSize, rows.length);
  const goToPage = (next: number) => setPagination({ result, page: Math.max(0, Math.min(next, pageCount - 1)) });
  useEffect(() => {
    if (scroller.current) scroller.current.scrollTop = 0;
  }, [result, page, pageSize]);
  const widths = useMemo(
    () => (result ? result.columns.map((name, i) => columnWidth(name, result.rows, i)) : []),
    [result],
  );
  const virtualizer = useVirtualizer({
    count: end - start,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
  });

  if (!result) {
    return <div className="grid-empty">Run a query to see its rows here.</div>;
  }
  if (result.columns.length === 0) {
    return <div className="grid-empty">The statement returned no rows.</div>;
  }

  const total = GUTTER_WIDTH + widths.reduce((a, b) => a + b, 0);
  const template = `${GUTTER_WIDTH}px ${widths.map((w) => `${w}px`).join(" ")}`;

  return (
    <div className="paginated-results">
    <div className="grid" ref={scroller} role="grid" aria-label="Query results" aria-rowcount={rows.length + 1}>
      <div className="grid-inner tw:min-w-full" style={{ width: total }}>
        <div className="grid-head" role="row" style={{ gridTemplateColumns: template, height: ROW_HEIGHT + 4 }}>
          <div className="grid-cell grid-gutter" role="columnheader" />
          {result.columns.map((name, i) => (
            <div className="grid-cell" role="columnheader" key={i} title={name}>
              {name}
            </div>
          ))}
        </div>
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((item) => (
            <div
              key={item.key}
              role="row"
              aria-rowindex={start + item.index + 2}
              className={item.index % 2 ? "grid-row odd" : "grid-row"}
              style={{
                gridTemplateColumns: template,
                height: item.size,
                transform: `translateY(${item.start}px)`,
              }}
            >
              <div className="grid-cell grid-gutter" role="rowheader">
                {start + item.index + 1}
              </div>
              {(rows[start + item.index] ?? []).map((cell, c) => {
                const label = cellLabel(cell);
                const cls = `grid-cell${label.dim ? " dim" : ""}${label.numeric ? " num" : ""}`;
                return (
                  <div className={cls} role="gridcell" key={c} title={label.text}>
                    {label.text}
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      </div>
    </div>
    <nav className="result-pagination" aria-label="Result pagination">
      <div className="result-page-size"><span>Rows per page</span><SelectField aria-label="Rows per page" value={String(pageSize)} options={pageSizes} onValueChange={value => { setPageSize(Number(value)); setPagination({ result, page: 0 }); }} /></div>
      <span className="result-row-range" role="status">{rows.length ? `${(start + 1).toLocaleString()}–${end.toLocaleString()}` : "0"} of {rows.length.toLocaleString()} rows</span>
      <div className="result-page-controls">
        <span>Page {page + 1} of {pageCount.toLocaleString()}</span>
        <button type="button" className="icon-button" aria-label="First page" title="First page" disabled={page === 0} onClick={() => goToPage(0)}><Icon name="chevronLeft" size={12} /><Icon name="chevronLeft" size={12} /></button>
        <button type="button" className="icon-button" aria-label="Previous page" title="Previous page" disabled={page === 0} onClick={() => goToPage(page - 1)}><Icon name="chevronLeft" size={14} /></button>
        <button type="button" className="icon-button" aria-label="Next page" title="Next page" disabled={page >= pageCount - 1} onClick={() => goToPage(page + 1)}><Icon name="chevronRight" size={14} /></button>
        <button type="button" className="icon-button" aria-label="Last page" title="Last page" disabled={page >= pageCount - 1} onClick={() => goToPage(pageCount - 1)}><Icon name="chevronRight" size={12} /><Icon name="chevronRight" size={12} /></button>
      </div>
    </nav>
    </div>
  );
}
