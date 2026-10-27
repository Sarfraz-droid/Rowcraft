import {
  FilePlusIcon, ChevronDownIcon, ChevronRightIcon, ChevronLeftIcon, PlusIcon, MinusIcon, MagnifyingGlassIcon, DashboardIcon,
  CodeIcon, Share2Icon, TableIcon, CounterClockwiseClockIcon, ReloadIcon, PlayIcon, StopIcon,
  DownloadIcon, SunIcon, MoonIcon, Cross2Icon, HamburgerMenuIcon, GearIcon,
  FileTextIcon, MixerHorizontalIcon, CheckIcon, Link2Icon, CopyIcon, DotsHorizontalIcon,
  BarChartIcon, ArrowRightIcon, MagicWandIcon, DoubleArrowDownIcon, DoubleArrowUpIcon,
} from "@radix-ui/react-icons";

const icons = {
  sidebarCollapse: ({ width, height, className }: { width?: number; height?: number; className?: string }) => <svg width={width} height={height} className={className} viewBox="0 0 15 15" fill="none" stroke="currentColor" strokeWidth="1"><rect x="1.5" y="2" width="12" height="11" rx="1" /><path d="M5 2v11m5-8-2.5 2.5L10 10" /></svg>,
  sidebarExpand: ({ width, height, className }: { width?: number; height?: number; className?: string }) => <svg width={width} height={height} className={className} viewBox="0 0 15 15" fill="none" stroke="currentColor" strokeWidth="1"><rect x="1.5" y="2" width="12" height="11" rx="1" /><path d="M5 2v11m3-8 2.5 2.5L8 10" /></svg>,
  newFile: FilePlusIcon,
  saveAll: ({ width, height, className }: { width?: number; height?: number; className?: string }) => <svg width={width} height={height} className={className} viewBox="0 0 15 15" fill="none" stroke="currentColor" strokeWidth="1"><path d="M4.5 1.5h7l2 2v8h-9zM7 1.5v4h4v-4M7 11.5v-4h4v4M2 4v9.5h9" /></svg>,
  closeAll: ({ width, height, className }: { width?: number; height?: number; className?: string }) => <svg width={width} height={height} className={className} viewBox="0 0 15 15" fill="none" stroke="currentColor" strokeWidth="1"><rect x="4.5" y="1.5" width="9" height="10" rx="1" /><path d="m7 4 4 5m0-5L7 9M2 4v8.5a1 1 0 0 0 1 1h8" /></svg>,
  ai: MagicWandIcon,
  expandAll: DoubleArrowDownIcon, collapseAll: DoubleArrowUpIcon,
  chevronDown: ChevronDownIcon, chevronRight: ChevronRightIcon, chevronLeft: ChevronLeftIcon, plus: PlusIcon, minus: MinusIcon,
  search: MagnifyingGlassIcon, dashboard: DashboardIcon, console: CodeIcon,
  map: Share2Icon, database: TableIcon, table: TableIcon, history: CounterClockwiseClockIcon,
  refresh: ReloadIcon, play: PlayIcon, stop: StopIcon, download: DownloadIcon, sun: SunIcon,
  moon: MoonIcon, close: Cross2Icon, menu: HamburgerMenuIcon, settings: GearIcon,
  file: FileTextIcon, filter: MixerHorizontalIcon, check: CheckIcon, link: Link2Icon,
  copy: CopyIcon, more: DotsHorizontalIcon, chart: BarChartIcon, arrow: ArrowRightIcon,
};
export type IconName = keyof typeof icons;
export function Icon({ name, size = 15, className = "" }: { name: IconName; size?: number; className?: string }) {
  const Glyph = icons[name];
  return <Glyph width={size} height={size} className={`icon ${className}`} aria-hidden="true" />;
}
