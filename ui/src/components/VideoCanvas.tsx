import { useRef, useEffect, useCallback } from "react";
import type { InputEvent } from "../types/ipc";

interface VideoCanvasProps {
  onInput: (event: InputEvent) => void;
}

/**
 * VideoCanvas with H.264 decoding via WebCodecs API.
 * Falls back to raw BGRA texture upload when WebCodecs is unavailable.
 */
export function VideoCanvas({ onInput }: VideoCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const glRef = useRef<WebGLRenderingContext | null>(null);
  const textureRef = useRef<WebGLTexture | null>(null);
  const programRef = useRef<WebGLProgram | null>(null);
  const decoderRef = useRef<VideoDecoder | null>(null);
  const frameBufferRef = useRef<Uint8Array | null>(null);

  // Initialize WebGL context and shaders
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const gl = canvas.getContext("webgl", { alpha: false, antialias: false });
    if (!gl) return;
    glRef.current = gl;

    // Vertex shader
    const vsSource = `
      attribute vec2 a_position;
      attribute vec2 a_texCoord;
      varying vec2 v_texCoord;
      void main() {
        gl_Position = vec4(a_position, 0.0, 1.0);
        v_texCoord = a_texCoord;
      }
    `;

    // Fragment shader for YUV→RGB conversion (WebCodecs outputs I420)
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

    // Full-screen quad
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

    // Create texture
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
            // Copy decoded frame to buffer for texture upload
            const width = frame.displayWidth;
            const height = frame.displayHeight;
            const bufSize = width * height * 4;

            if (!frameBufferRef.current || frameBufferRef.current.length !== bufSize) {
              frameBufferRef.current = new Uint8Array(bufSize);
            }

            // Copy RGBA data from VideoFrame
            frame.copyTo(frameBufferRef.current, { format: "RGBA" }).then(() => {
              uploadTexture(gl, texture!, frameBufferRef.current!, width, height);
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
          codec: "avc1.42E01E", // H.264 Baseline Level 3.0
          optimizeForLatency: true,
        });

        decoderRef.current = decoder;
        console.info("WebCodecs H.264 decoder initialized");
      } catch (e) {
        console.warn("WebCodecs init failed, using raw BGRA fallback:", e);
      }
    } else {
      console.info("WebCodecs not available, using raw BGRA fallback");
    }

    return () => {
      decoderRef.current?.close();
      decoderRef.current = null;
    };
  }, []);

  // Upload BGRA/RGBA data to WebGL texture
  const uploadTexture = useCallback(
    (gl: WebGLRenderingContext, texture: WebGLTexture, data: Uint8Array, width: number, height: number) => {
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, data);
    },
    [],
  );

  /**
   * Process incoming frame data from the relay.
   * If WebCodecs decoder is active, feeds NAL units to it.
   * Otherwise uploads raw BGRA data directly as texture.
   */
  const processFrame = useCallback(
    (data: Uint8Array, width: number, height: number, isKeyframe: boolean) => {
      const decoder = decoderRef.current;

      if (decoder && decoder.state !== "closed") {
        // Feed H.264 NAL units to WebCodecs decoder
        const chunk = new EncodedVideoChunk({
          timestamp: performance.now() * 1000,
          type: isKeyframe ? "key" : "delta",
          data,
        });

        try {
          decoder.decode(chunk);
        } catch (e) {
          console.warn("Decode failed, falling back to raw upload:", e);
          uploadRawFallback(data, width, height);
        }
      } else {
        // Raw BGRA fallback
        uploadRawFallback(data, width, height);
      }
    },
    [uploadTexture],
  );

  const uploadRawFallback = useCallback(
    (data: Uint8Array, width: number, height: number) => {
      const gl = glRef.current;
      const texture = textureRef.current;
      if (!gl || !texture) return;

      // BGRA → RGBA swizzle on CPU (WebGL doesn't support BGRA natively everywhere)
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

  // Expose processFrame to parent via ref or IPC callback
  useEffect(() => {
    // Register global handler for incoming frame data from Tauri IPC
    const handler = (event: CustomEvent<{ data: Uint8Array; width: number; height: number; isKeyframe: boolean }>) => {
      processFrame(event.detail.data, event.detail.width, event.detail.height, event.detail.isKeyframe);
    };

    window.addEventListener("cntdesk-frame" as string, handler as EventListener);
    return () => {
      window.removeEventListener("cntdesk-frame" as string, handler as EventListener);
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