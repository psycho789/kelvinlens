export const FUSE_HEADER_BYTES = 40;
export type Quad = [number, number, number, number];
export interface FuseHeader { width: number; height: number; source: Quad; destination: Quad }
export function parseFuseHeader(buf: ArrayBuffer): FuseHeader {
  const head = new DataView(buf, 0, FUSE_HEADER_BYTES);
  const f = (i: number) => head.getFloat32(8 + i * 4, true);
  return { width: head.getUint32(0, true), height: head.getUint32(4, true),
    source: [f(0), f(1), f(2), f(3)], destination: [f(4), f(5), f(6), f(7)] };
}
