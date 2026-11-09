// A sample that exercises the TypeScript grammar's highlight captures.
import type { Readable } from "node:stream";
import { join } from 'node:path';

/* A block comment
   across lines. */
export interface Point<T extends number = number> {
  readonly x: T;
  y?: T;
}

type Mode = "read" | "write";

export enum Colour {
  Red = 1,
  Green,
  Blue = "blue",
}

declare module "store" {
  export function open(name: string): Promise<void>;
}

namespace Shapes {
  export const unit: Point = { x: 1, y: 1 };
}

export abstract class Store<K extends PropertyKey, V> implements Iterable<V> {
  private readonly items = new Map<K, V>();
  protected static count = 0;

  constructor(public name: string, private mode: Mode = "read") {}

  abstract load(stream: Readable): Promise<V[]>;

  get(key: K): V | undefined {
    return this.items.get(key) as V | undefined;
  }

  *[Symbol.iterator](): Iterator<V> {
    yield* this.items.values();
  }
}

function assertNever(value: never): never {
  throw new Error(`unexpected ${value satisfies never}`);
}

const total = [1, 2, 3].reduce((a: number, b) => a + b, 0) ?? 0x1f;
let file: string | null = join("a", "b");
const shape = <Point>{ x: 2 };

export async function main(args: readonly string[]): Promise<number> {
  for (const arg of args) {
    if (typeof arg === "string" && arg !== "") {
      console.log(arg!.length, file);
    }
  }
  return total;
}
