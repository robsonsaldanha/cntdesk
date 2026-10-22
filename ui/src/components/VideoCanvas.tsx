import { useRef, useEffect, useCallback } from "react";
import type { InputEvent } from "../types/ipc";

interface VideoCanvasProps {
  onInput: (event: InputEvent) => void;
}

/**
 * VideoCanvas — renders remote desktop frames.
 * v52+: JPEG compressed frames decoded via createImageBitmap.
 * Fallback: raw BGRA texture upload or WebCodecs H.264 decoder.
 */
export function VideoCanvas({ onInput }: VideoCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const glRef = useRef<WebGLRenderingContext | null>(null);
  const textureRef = useRef<WebGLTexture | null>(null);
  const programRef = useRef<WebGLProgram | null>(null);
  const decoderRef = useRef<VideoDecoder | null>(null);
  const frameBufferRef = useRef<Uint8Array | null>(null);

  // Upload BGRA/RGBA data to WebGL texture
  const uploadTexture = useCallback(
    (gl: WebGLRenderingContext, texture: WebGLTexture, data: Uint8Array, width: number, height: number) => {
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, data);
    },
    [],
  );

  // Raw BGRA fallback: swizzle BGRA→RGBA on CPU, upload to texture
  const uploadRawFallback = useCallback(
    (data: Uint8Array, width: number, height: number) => {
      const gl = glRef.current;
      const texture = textureRef.current;
      if (!gl || !texture) return;

      const rgba = new Uint8Array(data.length);
      for (let i = 0; i < data.length; i += 4) {
        rgba[i] = data[i + 2];     // R ← B
        rgba[i + 1] = data[i + 1]; // G ← G
        rgba[i + 2] = data[i];     // B ← R
        rgba[i + 3] = data[i + 3]; // A ← A
      }

      uploadTexture(gl, texture, rgba, width, height);
      gl.drawArrays(gl.TRIANGLES, 0, 6);
    },
    [uploadTexture],
  );

  // Process incoming frame data
  const processFrame = useCallback(
    async (data: Uint8Array, width: number, height: number, isKeyframe: boolean) => {
      // v52+ sends JPEG compressed frames (~200KB instead of 8MB raw BGRA).
      // Detect by checking for JPEG magic bytes (FF D8 FF).
      const isJPEG = data.length >= 3 && data[0] === 0xFF && data[1] === 0xD8 && data[2] === 0xFF;

      if (isJPEG) {
        try {
          // Cast Uint8Array to ArrayBuffer for Blob compatibility
          const blob = new Blob([data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength)], { type: "image/jpeg" });
          const bitmap = await createImageBitmap(blob);
          const gl = glRef.current;
          const texture = textureRef.current;
          if (gl && texture) {
            gl.bindTexture(gl.TEXTURE_2D, texture);
            gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, bitmap);
            gl.drawArrays(gl.TRIANGLES, 0, 6);
            bitmap.close();
          }
        } catch (e) {
          console.warn("JPEG decode failed:", e);
        }
      } else {
        // Fallback: raw BGRA or H.264
        const expectedRawSize = width * height * 4;
        const isRawBGRA = data.length === expectedRawSize;
        if (isRawBGRA) {
          uploadRawFallback(data, width, height);
        } else {
          const decoder = decoderRef.current;
          if (decoder && decoder.state !== "closed") {
            const chunk = new EncodedVideoChunk({
              timestamp: performance.now() * 1000,
              type: isKeyframe ? "key" : "delta",
              data,
            });
            try {
              decoder.decode(chunk);
            } catch (e) {
              console.warn("Decode failed:", e);
            }
          }
        }
      }
    },
    [uploadRawFallback],
  );

  // Initialize WebGL context and shaders
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const gl = canvas.getContext("webgl", { alpha: false, antialias: false });
    if (!gl) return;
    glRef.current = gl;

    const vsSource = `
      attribute vec2 a_position;
      attribute vec2 a_texCoord;
      varying vec2 v_texCoord;
      void main() {
        gl_Position = vec4(a_position, 0.0, 1.0);
        v_texCoord = a_texCoord;
      }
    `;

    const fsSource = `
      precision mediump float;
      varying vec2 v_texCoord;
      uniform sampler2D u_texture;
      void main() {
        gl_FragColor = texture2D(u_texture, v_texCoord);
      }
    `;

    const vs = gl.createShader(gl.VERTEX_SHADER)!;
    gl.shaderSource(vs, vsSource);
    gl.compileShader(vs);

    const fs = gl.createShader(gl.FRAGMENT_SHADER)!;
    gl.shaderSource(fs, fsSource);
    gl.compileShader(fs);

    const program = gl.createProgram()!;
    gl.attachShader(program, vs);
    gl.attachShader(program, fs);
    gl.linkProgram(program);
    gl.useProgram(program);
    programRef.current = program;

    const positions = new Float32Array([
      -1, -1,  1, -1, -1,  1,
      -1,  1,  1, -1,  1,  1,
    ]);
    const texCoords = new Float32Array([
      0, 1, 1, 1, 0, 0,
      0, 0, 1, 1, 1, 0,
    ]);

    const posBuf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, posBuf);
    gl.bufferData(gl.ARRAY_BUFFER, positions, gl.STATIC_DRAW);
    const posLoc = gl.getAttribLocation(program, "a_position");
    gl.enableVertexAttribArray(posLoc);
    gl.vertexAttribPointer(posLoc, 2, gl.FLOAT, false, 0, 0);

    const texBuf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, texBuf);
    gl.bufferData(gl.ARRAY_BUFFER, texCoords, gl.STATIC_DRAW);
    const texLoc = gl.getAttribLocation(program, "a_texCoord");
    gl.enableVertexAttribArray(texLoc);
    gl.vertexAttribPointer(texLoc, 2, gl.FLOAT, false, 0, 0);

    const texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    textureRef.current = texture;

    // Initialize WebCodecs decoder if available
    if ("VideoDecoder" in window) {
      try {
        const decoder = new VideoDecoder({
          output: (frame: VideoFrame) => {
            const w = frame.displayWidth;
            const h = frame.displayHeight;
            const bufSize = w * h * 4;

            if (!frameBufferRef.current || frameBufferRef.current.length !== bufSize) {
              frameBufferRef.current = new Uint8Array(bufSize);
            }

            frame.copyTo(frameBufferRef.current!, { format: "RGBA" }).then(() => {
              uploadTexture(gl, texture!, frameBufferRef.current!, w, h);
              gl.drawArrays(gl.TRIANGLES, 0, 6);
              frame.close();
            }).catch((err: unknown) => {
              console.warn("Frame copy failed:", err);
              frame.close();
            });
          },
          error: (err: DOMException) => {
            console.error("VideoDecoder error:", err.message);
          },
        });

        decoder.configure({
          codec: "avc1.42E01E",
          optimizeForLatency: true,
        });

        decoderRef.current = decoder;
        console.info("WebCodecs H.264 decoder initialized");
      } catch (e) {
        console.warn("WebCodecs init failed, using fallback:", e);
      }
    } else {
      console.info("WebCodecs not available, using fallback");
    }

    return () => {
      decoderRef.current?.close();
      decoderRef.current = null;
    };
  }, [uploadTexture]);

  // Listen for Tauri "cntdesk-frame" events
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlisten = await listen<{
          data: number[];
          width: number;
          height: number;
          isKeyframe: boolean;
        }>("cntdesk-frame", (event) => {
          const { data, width, height, isKeyframe } = event.payload;
          processFrame(new Uint8Array(data), width, height, isKeyframe);
        });
      } catch (e) {
        console.warn("Tauri event listener init failed:", e);
      }
    })();

    return () => {
      unlisten?.();
    };
  }, [processFrame]);

  // Mouse/keyboard input capture
  const handleMouseMove = useCallback(
    (e: React.MouseEvent) => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const rect = canvas.getBoundingClientRect();
      const x = Math.round(e.clientX - rect.left);
      const y = Math.round(e.clientY - rect.top);
      const data = new Uint8Array(4);
      new DataView(data.buffer).setUint16(0, x, true);
      new DataView(data.buffer).setUint16(2, y, true);
      onInput({
        event_type: "MouseMove",
        timestamp_us: Math.round(performance.now() * 1000),
        data: Array.from(data),
      });
    },
    [onInput],
  );

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      onInput({
        event_type: "MouseDown",
        timestamp_us: Math.round(performance.now() * 1000),
        data: [e.button === 2 ? 1 : 0],
      });
    },
    [onInput],
  );

  const handleMouseUp = useCallback(
    (e: React.MouseEvent) => {
      onInput({
        event_type: "MouseUp",
        timestamp_us: Math.round(performance.now() * 1000),
        data: [e.button === 2 ? 1 : 0],
      });
    },
    [onInput],
  );

  const handleWheel = useCallback(
    (e: React.WheelEvent) => {
      const delta = Math.round(-e.deltaY);
      const data = new Uint8Array(2);
      new DataView(data.buffer).setInt16(0, delta, true);
      onInput({
        event_type: "Scroll",
        timestamp_us: Math.round(performance.now() * 1000),
        data: Array.from(data),
      });
    },
    [onInput],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const data = new Uint8Array(2);
      new DataView(data.buffer).setUint16(0, e.keyCode, true);
      onInput({
        event_type: "KeyDown",
        timestamp_us: Math.round(performance.now() * 1000),
        data: Array.from(data),
      });
    },
    [onInput],
  );

  const handleKeyUp = useCallback(
    (e: React.KeyboardEvent) => {
      const data = new Uint8Array(2);
      new DataView(data.buffer).setUint16(0, e.keyCode, true);
      onInput({
        event_type: "KeyUp",
        timestamp_us: Math.round(performance.now() * 1000),
        data: Array.from(data),
      });
    },
    [onInput],
  );

  return (
    <canvas
      ref={canvasRef}
      style={{ width: "100%", height: "100%", display: "block" }}
      width={1920}
      height={1080}
      onMouseMove={handleMouseMove}
      onMouseDown={handleMouseDown}
      onMouseUp={handleMouseUp}
      onWheel={handleWheel}
      onKeyDown={handleKeyDown}
      onKeyUp={handleKeyUp}
      tabIndex={0}
    />
  );
}