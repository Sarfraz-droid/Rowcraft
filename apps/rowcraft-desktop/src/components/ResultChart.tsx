import { SelectField } from "./SelectField";
import { useState } from "react";
import type { QueryOutput } from "../api";
import { chartPoints, numericColumns } from "../result-tools";
import { Icon } from "./Icon";

export function ResultChart({ result }: { result: QueryOutput | null }) {
  const [labelChoice, setLabelChoice] = useState("");
  const [valueChoice, setValueChoice] = useState("");
  if (!result || !result.rows.length) return <div className="result-chart-empty"><Icon name="chart" size={26} /><strong>Turn query results into a chart</strong><p>Run a query with a numeric column, then choose a label and value.</p></div>;
  const numeric = numericColumns(result);
  if (!numeric.length) return <div className="result-chart-empty"><Icon name="chart" size={26} /><strong>No numeric columns in this result</strong><p>Select a number such as a count, total, or average in your SQL query.</p></div>;
  const selectedValue = Number(valueChoice);
  const valueIndex = valueChoice !== "" && numeric.includes(selectedValue) ? selectedValue : numeric[0]!;
  const selectedLabel = Number(labelChoice);
  const labelIndex = labelChoice !== "" && selectedLabel >= -1 && selectedLabel < result.columns.length ? selectedLabel : result.columns.findIndex((_, index) => !numeric.includes(index));
  const points = chartPoints(result, labelIndex, valueIndex);
  const eligible = result.rows.filter(row => typeof row[valueIndex] === "number" && Number.isFinite(row[valueIndex])).length;
  const min = Math.min(0, ...points.map(point => point.value));
  const max = Math.max(0, ...points.map(point => point.value));
  const scale = Math.max(Math.abs(min), Math.abs(max)) || 1;
  const lower = min / scale;
  const range = max / scale - lower || 1;
  const x = (value: number) => 205 + ((value / scale - lower) / range) * 530;
  const zero = x(0);
  return <section className="result-chart" aria-label="Query result chart"><div className="chart-controls"><label>Label<SelectField aria-label="Chart label" value={String(labelIndex)} onValueChange={setLabelChoice} options={[{ value: "-1", label: "Row number" }, ...result.columns.map((column, index) => ({ value: String(index), label: column }))]} /></label><label>Value<SelectField aria-label="Chart value" value={String(valueIndex)} onValueChange={setValueChoice} options={numeric.map(index => ({ value: String(index), label: result.columns[index] }))} /></label><span>{points.length} of {eligible} numeric rows{result.rows.length !== eligible ? ` · ${result.rows.length - eligible} omitted` : ""}</span></div><div className="chart-scroll tw:overflow-auto tw:[flex:1] tw:min-h-0 tw:[padding:12px_16px]"><svg className="bar-chart tw:w-full tw:min-w-[500px] tw:max-w-[1100px] tw:block" viewBox={`0 0 840 ${points.length * 34 + 46}`} role="img" aria-label={`${result.columns[valueIndex]} by ${labelIndex < 0 ? "row number" : result.columns[labelIndex]}, first ${points.length} numeric rows`}><line x1={zero} x2={zero} y1={12} y2={points.length * 34 + 12} className="chart-baseline tw:stroke-border tw:[stroke-width:1]" />{points.map((point, index) => <g key={point.row}><title>{point.label}: {point.value}</title><text x={190} y={index * 34 + 32} textAnchor="end" className="chart-label">{point.label.length > 23 ? `${point.label.slice(0, 21)}…` : point.label}</text><rect x={Math.min(zero, x(point.value))} y={index * 34 + 16} width={Math.max(1, Math.abs(x(point.value) - zero))} height={22} rx={2} className={point.value < 0 ? "chart-bar negative" : "chart-bar"} /><text x={830} y={index * 34 + 32} textAnchor="end" className="chart-value">{new Intl.NumberFormat(undefined, { maximumSignificantDigits: 5 }).format(point.value)}</text></g>)}</svg></div><p className="chart-caption tw:text-muted-foreground tw:text-meta tw:[padding:9px_20px] tw:m-0 tw:[border-top:1px_solid_var(--border)]">Values are plotted per returned row, without aggregation.{eligible > 50 ? " Showing the first 50 numeric rows." : ""} Hover a bar for its exact value.</p></section>;
}
