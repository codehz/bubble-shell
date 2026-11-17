// ============================================================================
// Reactive primitives
// ============================================================================

export interface Signal<T> {
  (): T;
  update(fn: (prev: T) => T): void;
  set(value: T): void;
}

export function signal<T>(
  initial: T,
  options?: { equals?: boolean | ((a: T, b: T) => boolean) }
): Signal<T>;

export function computed<T>(
  run: () => T,
  options?: { equals?: boolean | ((a: T, b: T) => boolean) }
): Signal<T>;

export function effect(run: () => void | (() => void)): void;
export function cleanup(fn: () => void): void;
export function batch<T>(fn: () => T): T;

export function mount(component: () => void, retained?: boolean): void;

// ============================================================================
// Control flow (compiler-lowered, used as JSX tags)
// ============================================================================

export const Show: <T>(props: {
  when: T | (() => T);
  fallback?: unknown;
  children: (value: T) => void;
}) => void;

export const For: <T>(props: {
  each: T[] | (() => T[]);
  by: (item: T, index: number) => string | number;
  children: (item: T, index: number) => void;
}) => void;

export const Index: <T>(props: {
  each: T[] | (() => T[]);
  children: (item: () => T, index: number) => void;
}) => void;

export const Switch: (props: {
  fallback?: unknown;
  children: () => void;
}) => void;

export const Match: (props: {
  when: boolean | (() => boolean);
  children: () => void;
}) => void;

export const Slot: (props: {
  of?: () => void;
  fallback?: unknown;
  children?: () => void;
}) => void;

// ============================================================================
// Style declarations (as JSX children)
// ============================================================================

export const ViewStyle: (props: {
  target: unknown;
  when?: unknown;
  backgroundColor?: unknown;
  radius?: unknown;
  opacity?: unknown;
  fit?: unknown;
  translateX?: unknown;
  translateY?: unknown;
  scale?: unknown;
  rotate?: unknown;
}) => void;

export const TextStyle: (props: {
  target: unknown;
  when?: unknown;
  color?: unknown;
  textAlign?: unknown;
}) => void;

// ============================================================================
// Shared values and animation
// ============================================================================

export interface Ref<T = number> {
  current: T;
}

export function ref<T = number>(initial?: T): Ref<T>;

export interface SharedValue<T = number> {
  value: T;
  set(value: T): void;
  bind(signal: Signal<T>): void;
  settle(): Promise<void>;
}

export function createSharedValue<T = number>(
  valueOrOptions: T | { source?: string; target?: Ref; value?: T },
  options?: { value?: T }
): SharedValue<T>;

export function withTiming(
  target: number,
  options?: { duration?: number; easing?: "linear" | "easeInOut" }
): number;

export function withSpring(
  target: number,
  options?: { stiffness?: number; damping?: number }
): number;

export function cancelAnimation(sv: SharedValue): void;

// ============================================================================
// UI worklets
// ============================================================================

export function scheduleOnUI<T extends unknown[]>(
  fn: (...args: T) => void,
  ...args: T
): void;

export function scheduleOnJS<T extends unknown[]>(
  fn: (...args: T) => void,
  ...args: T
): void;
