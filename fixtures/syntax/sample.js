// A sample that exercises the JavaScript grammar's highlight captures.
import { readFile } from "node:fs/promises";
import * as path from 'node:path';

/* A block comment
   across lines. */
const LIMIT = 10;
let count = 0x1f + 1e3 + 0b101;
var legacy = null ?? undefined;

/**
 * Sums the numbers in a list.
 */
export function sum(values) {
  return values.reduce((total, value) => total + value, 0);
}

export default class Store extends Map {
  #secret = true;
  static instances = 0;

  constructor(name) {
    super();
    this.name = name;
    Store.instances++;
  }

  get size() {
    return super.size;
  }

  async load(file) {
    try {
      const text = await readFile(path.join(".", file), "utf8");
      return JSON.parse(text);
    } catch (error) {
      console.error(`could not load ${file}: ${error.message}`);
      throw new Error("load failed");
    } finally {
      count += 1;
    }
  }
}

function* ids(start = 0) {
  for (let i = start; i < LIMIT; i++) {
    if (i % 2 === 0 && i !== 4) yield i;
    else continue;
  }
}

const pattern = /^[a-z]+\d*$/gi;
const { a, ...rest } = { a: 1, b: [true, false] };
const view = <div className="box">{a}</div>;

switch (typeof rest) {
  case "object":
    break;
  default:
    delete rest.b;
}
