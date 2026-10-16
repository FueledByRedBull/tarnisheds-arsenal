// The suite runs in Node, but the project has no @types/node; these are the Node APIs it uses.
declare const process: { env: Record<string, string | undefined> };
declare class Buffer extends Uint8Array {
  static from(data: ArrayBuffer | ArrayLike<number> | string): Buffer;
  static concat(chunks: readonly Uint8Array[]): Buffer;
  toString(): string;
}
declare module "node:fs" {
  export function readFileSync(path: string | URL, encoding: "utf8"): string;
}
