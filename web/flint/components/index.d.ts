import type * as React from 'react';

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  /** primary: the one off-white action per region. secondary (default): stroke-strong edge. ghost: no edge. danger: destructive. */
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
  /** md is 32px tall (default); sm is 28px for toolbars and table rows. */
  size?: 'md' | 'sm';
  children?: React.ReactNode;
}
export declare function Button(props: ButtonProps): React.ReactElement;

export interface InputProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'size'> {
  /** Field label, rendered in mono caps above the well. Keep it to three words. */
  label?: React.ReactNode;
  /** Helper text under the field, in ink-subtle. */
  hint?: React.ReactNode;
  /** An error message. Replaces the hint, turns the edge danger and sets aria-invalid. */
  error?: React.ReactNode;
}
export declare function Input(props: InputProps): React.ReactElement;

export interface TagProps {
  /** neutral (default) draws a stroke edge; status tones sit on their tint. */
  tone?: 'neutral' | 'success' | 'warning' | 'danger';
  /** The 6px square marker. Defaults to on for status tones, off for neutral. */
  marker?: boolean;
  /** One to three words. Always present — colour never carries meaning alone. */
  children: React.ReactNode;
  className?: string;
}
export declare function Tag(props: TagProps): React.ReactElement;

export interface CardProps extends Omit<React.HTMLAttributes<HTMLElement>, 'title'> {
  /** A mono-caps line above the title: the kind of thing this is. */
  eyebrow?: React.ReactNode;
  /** The card's heading, set in `heading`. */
  title?: React.ReactNode;
  /** Right side of the header: usually one Tag. */
  meta?: React.ReactNode;
  /** Actions, right-aligned under a stroke-subtle rule. */
  footer?: React.ReactNode;
  /** The whole card is one target: hover lifts the edge to stroke-strong. Contains no other controls. */
  interactive?: boolean;
  /** A chosen card: stroke-bright edge. */
  selected?: boolean;
  children?: React.ReactNode;
}
export declare function Card(props: CardProps): React.ReactElement;

export interface TabItem {
  id: string;
  label: React.ReactNode;
  /** A number shown in mono after the label. */
  count?: number | string;
  disabled?: boolean;
}
export interface TabsProps {
  items: TabItem[];
  /** Controlled selection. */
  value?: string;
  /** Uncontrolled initial selection; defaults to the first item. */
  defaultValue?: string;
  onChange?: (id: string) => void;
  /** Accessible name for the tab list. */
  label?: string;
  className?: string;
}
export declare function Tabs(props: TabsProps): React.ReactElement;

export type MenuEntry =
  | { type?: 'item'; label: React.ReactNode; shortcut?: string; tone?: 'danger'; disabled?: boolean; onSelect?: () => void }
  | { type: 'separator' }
  | { type: 'label'; label: React.ReactNode };
export interface MenuProps {
  items: MenuEntry[];
  /** Called with the chosen entry, after its own onSelect. */
  onSelect?: (entry: MenuEntry) => void;
  /** Accessible name for the menu. */
  label?: string;
  className?: string;
  style?: React.CSSProperties;
}
export declare function Menu(props: MenuProps): React.ReactElement;

export interface BarSeries {
  /** Shown in the legend, the tooltip and the table view. */
  name: string;
  /** One value per category, in category order. Negative or missing values draw as 0. */
  values: number[];
  /** Pin this series to a chart colour (1–5 = chart-1…chart-5) so filtering never repaints it; 'other' draws in ink-subtle. Defaults to its position. */
  slot?: 1 | 2 | 3 | 4 | 5 | 'other';
}
export interface BarChartProps {
  /** The category labels: the x axis when vertical, the row labels when horizontal. */
  categories: string[];
  /** One to five series. One series draws in ink-muted with no legend; more take chart-1… in order. */
  series: BarSeries[];
  /** vertical (default): columns from a bottom baseline. horizontal: bars from a left baseline — use it for long category names. */
  orientation?: 'vertical' | 'horizontal';
  /** Stack the series into one bar per category (part-to-whole). The tooltip adds a total. */
  stacked?: boolean;
  /** Accessible name for the chart and caption of its table view. */
  label?: string;
  /** Header for the category column in the table view. Defaults to "Category". */
  categoryLabel?: string;
  /** Plot height in px for vertical charts. Defaults to 200. Horizontal charts size to their rows. */
  height?: number;
  /** Force the value axis maximum (rounded up to a clean tick). */
  max?: number;
  /** Formats axis ticks and tooltip values. Defaults to a locale number with at most one decimal. */
  formatValue?: (value: number) => string;
  /** Show the legend. Defaults to on for two or more series, off for one. */
  legend?: boolean;
  className?: string;
  style?: React.CSSProperties;
}
export declare function BarChart(props: BarChartProps): React.ReactElement;

declare global {
  interface Window {
    Flint: { Button: typeof Button; Input: typeof Input; Tag: typeof Tag; Card: typeof Card; Tabs: typeof Tabs; Menu: typeof Menu; BarChart: typeof BarChart };
  }
}
