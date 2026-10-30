import { Component, type ReactNode } from 'react';

/** Keeps a failing part (one widget, one panel) from taking the whole app down: it shows `fallback` instead, and tries again when `resetKey` changes. */
export class Boundary extends Component<{ children: ReactNode; fallback?: ReactNode; resetKey?: unknown }, { error: Error | null; key: unknown }> {
  state = { error: null as Error | null, key: this.props.resetKey };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  static getDerivedStateFromProps(p: { resetKey?: unknown }, s: { error: Error | null; key: unknown }) {
    return p.resetKey !== s.key ? { error: null, key: p.resetKey } : null;
  }

  componentDidCatch(error: Error) {
    console.error(error);
  }

  render() {
    return this.state.error ? (this.props.fallback ?? null) : this.props.children;
  }
}
