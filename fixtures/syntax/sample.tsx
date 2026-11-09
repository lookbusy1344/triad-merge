// A sample that exercises the TSX grammar's highlight captures.
import React, { useState } from "react";
import type { ReactNode } from 'react';

/* A block comment
   across lines. */
export interface Props {
  readonly title: string;
  children?: ReactNode;
}

type Mode = "open" | "closed";

export enum Size {
  Small = 1,
  Large = "large",
}

export function Panel({ title, children }: Props): JSX.Element {
  const [mode, setMode] = useState<Mode>("open");
  const count: number = React.Children.count(children) ?? 0x1f;
  return (
    <section className="panel" data-mode={mode}>
      <h1 onClick={() => setMode(mode === "open" ? "closed" : "open")}>
        {title} ({count})
      </h1>
      {mode === "open" && <div>{children}</div>}
    </section>
  );
}

export abstract class Store<K extends PropertyKey, V> {
  private readonly items = new Map<K, V>();
  protected static count = 0;

  abstract load(): Promise<V[]>;

  get(key: K): V | undefined {
    return this.items.get(key) as V | undefined;
  }
}

const identity = <T,>(value: T): T => value;
let label: string | null = `size ${Size.Large}`;

export default function App(): JSX.Element {
  try {
    return <Panel title={label!}>{identity(1)}</Panel>;
  } catch (error: unknown) {
    throw new Error("render failed");
  }
}
