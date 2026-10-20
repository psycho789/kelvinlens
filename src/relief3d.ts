// 3D thermal relief: the image as a surface whose height is temperature, coloured with the
// current palette. Drag to turn, wheel to zoom, slider for height. WebGL, no libraries.
import { h, toast } from "./dom";
import { modal } from "./modal";
import { displayRange, lookOf, state, type Doc } from "./state";
import { symbol, temp } from "./units";

const VERT = `
attribute vec2 a_xy;
attribute float a_t;
uniform mat4 u_mvp;
uniform float u_height;
varying float v_t;
void main() {
  v_t = a_t;
  gl_Position = u_mvp * vec4(a_xy.x, a_t * u_height, a_xy.y, 1.0);
}`;

const FRAG = `
precision mediump float;
uniform sampler2D u_palette;
varying float v_t;
void main() {
  gl_FragColor = texture2D(u_palette, vec2(clamp(v_t, 0.0, 1.0), 0.5));
}`;

type Mat = Float32Array;

function multiply(a: Mat, b: Mat): Mat {
  const o = new Float32Array(16);
  for (let i = 0; i < 4; i++) for (let j = 0; j < 4; j++) for (let k = 0; k < 4; k++) o[j * 4 + i]! += a[k * 4 + i]! * b[j * 4 + k]!;
  return o;
}

function perspective(fov: number, aspect: number, near: number, far: number): Mat {
  const f = 1 / Math.tan(fov / 2);
  return new Float32Array([f / aspect, 0, 0, 0, 0, f, 0, 0, 0, 0, (far + near) / (near - far), -1, 0, 0, (2 * far * near) / (near - far), 0]);
}

function rotateX(a: number): Mat {
  const c = Math.cos(a);
  const s = Math.sin(a);
  return new Float32Array([1, 0, 0, 0, 0, c, s, 0, 0, -s, c, 0, 0, 0, 0, 1]);
}

function rotateY(a: number): Mat {
  const c = Math.cos(a);
  const s = Math.sin(a);
  return new Float32Array([c, 0, -s, 0, 0, 1, 0, 0, s, 0, c, 0, 0, 0, 0, 1]);
}

function translate(x: number, y: number, z: number): Mat {
  return new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, x, y, z, 1]);
}

function shader(gl: WebGLRenderingContext, type: number, src: string): WebGLShader {
  const s = gl.createShader(type)!;
  gl.shaderSource(s, src);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? "shader error");
  return s;
}

/** Builds the relief in `canvas`; returns a function that redraws with new settings. */
export function relief(canvas: HTMLCanvasElement, doc: Doc): { draw(yaw: number, pitch: number, zoom: number, height: number): void } {
  const gl = canvas.getContext("webgl", { antialias: true });
  if (!gl) throw new Error("3D view needs WebGL, which this system does not provide");
  const { width: w, height: hh } = doc.info;
  // At most ~256 cells per side, averaging blocks.
  const step = Math.max(1, Math.ceil(Math.max(w, hh) / 256));
  const gw = Math.floor((w - 1) / step) + 1;
  const gh = Math.floor((hh - 1) / step) + 1;
  const [lo, hi] = displayRange(doc);
  const span = hi - lo || 1;
  const aspect = hh / w;
  const xy = new Float32Array(gw * gh * 2);
  const t = new Float32Array(gw * gh);
  for (let j = 0; j < gh; j++) {
    for (let i = 0; i < gw; i++) {
      let sum = 0;
      let n = 0;
      for (let y = j * step; y < Math.min(hh, (j + 1) * step); y++) {
        for (let x = i * step; x < Math.min(w, (i + 1) * step); x++) {
          const v = doc.kelvin[y * w + x]!;
          if (Number.isFinite(v)) {
            sum += v;
            n++;
          }
        }
      }
      const k = j * gw + i;
      xy[k * 2] = (i / (gw - 1 || 1)) * 2 - 1;
      xy[k * 2 + 1] = ((j / (gh - 1 || 1)) * 2 - 1) * aspect;
      t[k] = n ? Math.min(1, Math.max(0, (sum / n - lo) / span)) : 0;
    }
  }
  const indices: number[] = [];
  for (let j = 0; j < gh - 1; j++) {
    for (let i = 0; i < gw - 1; i++) {
      const a = j * gw + i;
      indices.push(a, a + 1, a + gw, a + 1, a + gw + 1, a + gw);
    }
  }
  const prog = gl.createProgram()!;
  gl.attachShader(prog, shader(gl, gl.VERTEX_SHADER, VERT));
  gl.attachShader(prog, shader(gl, gl.FRAGMENT_SHADER, FRAG));
  gl.linkProgram(prog);
  gl.useProgram(prog);
  const buf = (data: Float32Array, name: string, size: number) => {
    const b = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, b);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
    const loc = gl.getAttribLocation(prog, name);
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, size, gl.FLOAT, false, 0, 0);
  };
  buf(xy, "a_xy", 2);
  buf(t, "a_t", 1);
  const ext = gl.getExtension("OES_element_index_uint");
  const idx = gl.createBuffer();
  gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, idx);
  const use32 = gw * gh > 65535;
  if (use32 && !ext) throw new Error("The image is too large for this system's 3D support");
  gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, use32 ? new Uint32Array(indices) : new Uint16Array(indices), gl.STATIC_DRAW);

  const pal = lookOf(doc).palette.colors;
  const tex = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, tex);
  const px = new Uint8Array(256 * 4);
  for (let i = 0; i < 256; i++) {
    const c = pal[Math.min(pal.length - 1, Math.floor((i * pal.length) / 256))]!;
    px.set([c[0], c[1], c[2], 255], i * 4);
  }
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 256, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, px);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  gl.enable(gl.DEPTH_TEST);

  const mvpLoc = gl.getUniformLocation(prog, "u_mvp");
  const heightLoc = gl.getUniformLocation(prog, "u_height");
  return {
    draw(yaw, pitch, zoom, height) {
      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.round(canvas.clientWidth * dpr);
      canvas.height = Math.round(canvas.clientHeight * dpr);
      gl.viewport(0, 0, canvas.width, canvas.height);
      gl.clearColor(0.05, 0.06, 0.09, 1);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      const p = perspective(Math.PI / 4, canvas.width / canvas.height, 0.1, 100);
      const view = multiply(translate(0, -0.15, -3.2 / zoom), multiply(rotateX(pitch), rotateY(yaw)));
      gl.uniformMatrix4fv(mvpLoc, false, multiply(p, view));
      gl.uniform1f(heightLoc, height);
      gl.drawElements(gl.TRIANGLES, indices.length, use32 ? gl.UNSIGNED_INT : gl.UNSIGNED_SHORT, 0);
    },
  };
}

export async function reliefDialog(doc: Doc): Promise<void> {
  const canvas = h("canvas", { class: "relief-canvas" });
  let yaw = -0.5;
  let pitch = 0.75;
  let zoom = 1;
  let height = 0.45;
  let view: ReturnType<typeof relief> | null = null;
  const redraw = () => view?.draw(yaw, pitch, zoom, height);
  const slider = h("input", { type: "range", min: "0", max: "1.5", step: "0.01", value: String(height), "aria-label": "Height", oninput: (e: Event) => ((height = Number((e.target as HTMLInputElement).value)), redraw()) });
  let drag: [number, number] | null = null;
  canvas.addEventListener("pointerdown", (e) => {
    drag = [e.clientX, e.clientY];
    canvas.setPointerCapture(e.pointerId);
  });
  canvas.addEventListener("pointermove", (e) => {
    if (!drag) return;
    yaw += (e.clientX - drag[0]) * 0.01;
    pitch = Math.min(1.5, Math.max(0.05, pitch + (e.clientY - drag[1]) * 0.01));
    drag = [e.clientX, e.clientY];
    redraw();
  });
  canvas.addEventListener("pointerup", () => (drag = null));
  canvas.addEventListener("wheel", (e) => {
    e.preventDefault();
    zoom = Math.min(4, Math.max(0.4, zoom * Math.exp(-e.deltaY * 0.0015)));
    redraw();
  }, { passive: false });
  const [lo, hi] = displayRange(doc);
  await modal(
    `3D relief: ${doc.info.name}`,
    [
      canvas,
      h("div", { class: "row" }, h("span", { class: "label" }, "Height"), slider, h("span", { class: "muted small" }, `${temp(lo, state.unit)} (low) to ${temp(hi, state.unit)} (high) · drag to turn, scroll to zoom · ${symbol(state.unit)}`)),
    ],
    [{ label: "Close", value: "close", primary: true }],
    {
      wide: true,
      onOpen: () => {
        try {
          view = relief(canvas, doc);
          requestAnimationFrame(redraw);
          new ResizeObserver(redraw).observe(canvas);
        } catch (e) {
          toast(e instanceof Error ? e.message : String(e), "error");
        }
      },
    },
  );
}
