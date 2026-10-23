import { Component, type ReactNode } from "react";

interface ErrorBoundaryProps {
  /** What to show once a child throws, with a way to render the children again. */
  fallback: (error: unknown, retry: () => void) => ReactNode;
  children: ReactNode;
}

interface ErrorBoundaryState {
  failed: boolean;
  error: unknown;
}

/**
 * Shows `fallback` instead of its children once rendering them throws.
 *
 * Without one, React unmounts the whole tree on a render error, which leaves
 * the window blank and, because it is undecorated, without its window
 * controls. React offers boundaries only as class components.
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  override state: ErrorBoundaryState = { failed: false, error: undefined };

  static getDerivedStateFromError(error: unknown): ErrorBoundaryState {
    return { failed: true, error };
  }

  private readonly retry = () => this.setState({ failed: false, error: undefined });

  override render() {
    return this.state.failed ? this.props.fallback(this.state.error, this.retry) : this.props.children;
  }
}
