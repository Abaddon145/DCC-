import { useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { FBXLoader } from "three/examples/jsm/loaders/FBXLoader.js";
import { OBJLoader } from "three/examples/jsm/loaders/OBJLoader.js";
import { MTLLoader } from "three/examples/jsm/loaders/MTLLoader.js";
import { STLLoader } from "three/examples/jsm/loaders/STLLoader.js";
import { RoomEnvironment } from "three/examples/jsm/environments/RoomEnvironment.js";
import { assetMediaUrl } from "../lib/api";
import type { AssetMedia } from "../types";

type Capture = () => Promise<Uint8Array>;
interface ModelViewerProps {
  media?: AssetMedia;
  sourceUrl?: string;
  sourceName?: string;
  thumbnailMode?: boolean;
  onReady?: (capture: Capture) => void;
  onError?: (message: string) => void;
  onSaveCover?: (bytes: Uint8Array) => Promise<void>;
}

export function resolveModelResource(baseUrl: string, relative: string): string | null {
  const path = relative.trim().replace(/\\/g, "/");
  if (!path || path.startsWith("/") || path.split("/").includes("..") || /^[a-z][a-z\d+.-]*:/i.test(path)) return null;
  try {
    const url = new URL(path, baseUrl);
    const base = new URL(baseUrl);
    const directory = base.pathname.slice(0, base.pathname.lastIndexOf("/") + 1);
    return url.origin === base.origin && url.pathname.startsWith(directory) ? url.href : null;
  } catch { return null; }
}

export function objMaterialPaths(contents: string): string[] {
  return contents.split(/\r?\n/).map(line => line.trim()).filter(line => /^mtllib\s+/i.test(line))
    .map(line => line.replace(/^mtllib\s+/i, "").trim()).filter(Boolean);
}

function disposeObject(object: THREE.Object3D) {
  const textures = new Set<THREE.Texture>();
  object.traverse(child => {
    if (!(child instanceof THREE.Mesh)) return;
    child.geometry?.dispose();
    const materials = Array.isArray(child.material) ? child.material : [child.material];
    for (const material of materials) {
      if (!material) continue;
      for (const value of Object.values(material)) if (value instanceof THREE.Texture) textures.add(value);
      material.dispose();
    }
  });
  textures.forEach(texture => texture.dispose());
}

export function ModelViewer({ media, sourceUrl, sourceName, thumbnailMode = false, onReady, onError, onSaveCover }: ModelViewerProps) {
  const host = useRef<HTMLDivElement>(null);
  const callbacks = useRef({ onReady, onError });
  callbacks.current = { onReady, onError };
  const [wireframe, setWireframe] = useState(false);
  const [grid, setGrid] = useState(!thumbnailMode);
  const [error, setError] = useState("");
  const [warning, setWarning] = useState("");
  const [saving, setSaving] = useState(false);
  const modelRef = useRef<THREE.Object3D | null>(null);
  const gridRef = useRef<THREE.GridHelper | null>(null);
  const resetRef = useRef<() => void>(() => undefined);
  const captureRef = useRef<Capture | null>(null);

  useEffect(() => {
    const container = host.current;
    if (!container) return;
    setError(""); setWarning("");
    let alive = true;
    let frame = 0;
    let renderer: THREE.WebGLRenderer;
    try { renderer = new THREE.WebGLRenderer({ antialias: true, preserveDrawingBuffer: true }); }
    catch { const message = "无法创建 3D 视口，请检查显卡或 WebView2 的硬件加速"; setError(message); callbacks.current.onError?.(message); return; }
    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x10141b);
    let environment: ReturnType<THREE.PMREMGenerator["fromScene"]> | null = null;
    try {
      const pmrem = new THREE.PMREMGenerator(renderer);
      const room = new RoomEnvironment();
      try { environment = pmrem.fromScene(room, .04); scene.environment = environment.texture; }
      finally { room.dispose(); pmrem.dispose(); }
    } catch { /* Keep direct lights as a safe fallback on weaker GPUs. */ }
    const camera = new THREE.PerspectiveCamera(45, 1, .01, 100000);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, thumbnailMode ? 1 : 2));
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    renderer.toneMappingExposure = 1.15;
    container.appendChild(renderer.domElement);
    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = !thumbnailMode;
    const gridHelper = new THREE.GridHelper(20, 20, 0x56616f, 0x28313c);
    gridHelper.name = "__grid";
    gridHelper.visible = !thumbnailMode;
    gridRef.current = gridHelper;
    scene.add(gridHelper, new THREE.HemisphereLight(0xffffff, 0x465368, 1.25));
    const key = new THREE.DirectionalLight(0xffffff, 1.8); key.position.set(5, 8, 6); scene.add(key);
    const fill = new THREE.DirectionalLight(0xb7c9e9, .55); fill.position.set(-5, 3, -4); scene.add(fill);
    const resize = () => { const { width, height } = container.getBoundingClientRect(); renderer.setSize(Math.max(1, width), Math.max(1, height), false); camera.aspect = Math.max(1, width) / Math.max(1, height); camera.updateProjectionMatrix(); };
    const observer = new ResizeObserver(resize); observer.observe(container); resize();
    const draw = () => { if (!alive) return; controls.update(); renderer.render(scene, camera); frame = requestAnimationFrame(draw); };
    if (thumbnailMode) renderer.render(scene, camera); else draw();
    const fit = (object: THREE.Object3D) => {
      const sphere = new THREE.Box3().setFromObject(object).getBoundingSphere(new THREE.Sphere());
      const radius = Number.isFinite(sphere.radius) ? Math.max(sphere.radius, .5) : 1;
      controls.target.copy(sphere.center);
      camera.near = Math.max(.001, radius / 1000);
      camera.far = Math.max(100, radius * 1000);
      camera.position.copy(sphere.center).add(new THREE.Vector3(radius * 1.8, radius * 1.3, radius * 1.8));
      camera.updateProjectionMatrix(); controls.update();
    };
    resetRef.current = () => modelRef.current && fit(modelRef.current);
    const capture: Capture = async () => {
      if (!alive || !modelRef.current) throw new Error("模型尚未加载完成");
      renderer.render(scene, camera);
      const blob = await new Promise<Blob>((resolve, reject) => renderer.domElement.toBlob(value => value ? resolve(value) : reject(new Error("无法捕获模型画面")), "image/webp", .88));
      return new Uint8Array(await blob.arrayBuffer());
    };
    captureRef.current = capture;
    const url = sourceUrl || (media ? assetMediaUrl(media.id, "original") : "");
    const ext = (sourceName || media?.originalName || "").split(".").pop()?.toLowerCase();
    const failedResources = new Set<string>();
    const manager = new THREE.LoadingManager();
    manager.onError = resource => {
      const label = resource.startsWith("blob:") ? `内嵌纹理 ${failedResources.size + 1}` : (resource.split("/").pop() || "未知纹理");
      failedResources.add(label);
      if (alive) setWarning(`部分材质或纹理未能加载：${[...failedResources].join("、")}`);
    };
    let ready = false;
    let resourcesReady = false;
    const maybeReady = () => {
      if (ready || !resourcesReady || !modelRef.current || !alive) return;
      ready = true;
      if (failedResources.size && thumbnailMode) {
        callbacks.current.onError?.(`材质或纹理缺失：${[...failedResources].join("、")}`);
        return;
      }
      requestAnimationFrame(() => { if (alive) callbacks.current.onReady?.(capture); });
    };
    manager.onLoad = () => { resourcesReady = true; maybeReady(); };
    const loaded = (object: THREE.Object3D, authoredMaterial: boolean) => {
      if (!alive) { disposeObject(object); return; }
      modelRef.current = object; scene.add(object); fit(object);
      if (!authoredMaterial && !failedResources.size) setWarning("源文件未提供材质，当前使用中性材质预览");
      maybeReady();
    };
    const failed = (reason: unknown) => {
      if (!alive) return;
      const message = `无法加载 3D 预览：${reason instanceof Error ? reason.message : String(reason)}`;
      setError(message); callbacks.current.onError?.(message);
    };
    const load = async () => {
      try {
        if (ext === "glb" || ext === "gltf") {
          const value = await new GLTFLoader(manager).loadAsync(url);
          loaded(value.scene, Boolean((value.parser.json.materials as unknown[] | undefined)?.length));
        } else if (ext === "fbx") {
          const object = await new FBXLoader(manager).loadAsync(url);
          let authored = false;
          object.traverse(child => { if (child instanceof THREE.Mesh && child.material) authored = true; });
          loaded(object, authored);
        } else if (ext === "obj") {
          const response = await fetch(url);
          if (!response.ok) throw new Error(`模型读取失败 (${response.status})`);
          const mtlPaths = objMaterialPaths(await response.text());
          let materials: MTLLoader.MaterialCreator | null = null;
          for (const path of mtlPaths) {
            const mtlUrl = resolveModelResource(url, path);
            if (!mtlUrl) { failedResources.add(path); continue; }
            try {
              const loader = new MTLLoader(manager);
              loader.setResourcePath(mtlUrl.slice(0, mtlUrl.lastIndexOf("/") + 1));
              const next = await loader.loadAsync(mtlUrl);
              next.preload();
              resourcesReady = false;
              if (materials) Object.assign(materials.materials, next.materials);
              else materials = next;
            } catch { failedResources.add(path); }
          }
          if (failedResources.size) setWarning(`部分材质或纹理未能加载：${[...failedResources].join("、")}`);
          const loader = new OBJLoader(manager);
          if (materials) loader.setMaterials(materials);
          resourcesReady = false;
          loaded(await loader.loadAsync(url), Boolean(materials));
        } else if (ext === "stl") {
          const geometry = await new STLLoader(manager).loadAsync(url);
          loaded(new THREE.Mesh(geometry, new THREE.MeshStandardMaterial({ color: 0xc5ccd6, roughness: .7 })), false);
        } else failed("不支持的 3D 格式");
      } catch (reason) { failed(reason); }
    };
    void load();
    return () => {
      alive = false; cancelAnimationFrame(frame); observer.disconnect(); controls.dispose();
      captureRef.current = null; resetRef.current = () => undefined;
      if (modelRef.current) { scene.remove(modelRef.current); disposeObject(modelRef.current); modelRef.current = null; }
      environment?.dispose();
      renderer.dispose(); renderer.forceContextLoss(); renderer.domElement.remove();
    };
  }, [media?.id, media?.originalName, sourceUrl, sourceName, thumbnailMode]);

  useEffect(() => { modelRef.current?.traverse(object => { if (object instanceof THREE.Mesh) { const materials = Array.isArray(object.material) ? object.material : [object.material]; materials.forEach(material => { if ("wireframe" in material) (material as THREE.MeshStandardMaterial).wireframe = wireframe; }); } }); }, [wireframe]);
  useEffect(() => { if (gridRef.current) gridRef.current.visible = grid; }, [grid]);
  const saveCover = async (automatic: boolean) => {
    if (!onSaveCover || !captureRef.current || saving) return;
    setSaving(true);
    try { if (automatic) resetRef.current(); await onSaveCover(await captureRef.current()); }
    catch (reason) { setError(`保存模型封面失败：${String(reason)}`); }
    finally { setSaving(false); }
  };
  return <div className={`model-viewer-shell ${thumbnailMode ? "thumbnail-render" : ""}`}>
    <div ref={host} className="model-viewer-canvas" />
    {!thumbnailMode && error && <div className="model-viewer-error">{error}</div>}
    {!thumbnailMode && !error && warning && <div className="model-viewer-warning">{warning}</div>}
    {!thumbnailMode && <div className="model-viewer-tools">
      <button onClick={() => resetRef.current()}>适应窗口</button>
      <button className={grid ? "active" : ""} onClick={() => setGrid(value => !value)}>网格</button>
      <button className={wireframe ? "active" : ""} onClick={() => setWireframe(value => !value)}>线框</button>
      {onSaveCover && <><button disabled={saving} onClick={() => void saveCover(false)}>设为封面</button><button disabled={saving} onClick={() => void saveCover(true)}>重新生成自动封面</button></>}
    </div>}
  </div>;
}
