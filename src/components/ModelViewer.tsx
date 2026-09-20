import { useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { FBXLoader } from "three/examples/jsm/loaders/FBXLoader.js";
import { OBJLoader } from "three/examples/jsm/loaders/OBJLoader.js";
import { STLLoader } from "three/examples/jsm/loaders/STLLoader.js";
import { assetMediaUrl } from "../lib/api";
import type { AssetMedia } from "../types";

export function ModelViewer({ media }: { media: AssetMedia }) {
  const host = useRef<HTMLDivElement>(null);
  const [wireframe, setWireframe] = useState(false);
  const [grid, setGrid] = useState(true);
  const [error, setError] = useState("");
  const modelRef = useRef<THREE.Object3D | null>(null);
  const gridRef = useRef<THREE.GridHelper | null>(null);
  const resetRef = useRef<() => void>(() => undefined);

  useEffect(() => {
    const container = host.current;
    if (!container) return;
    setError("");
    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x10141b);
    const camera = new THREE.PerspectiveCamera(45, 1, .01, 100000);
    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    container.appendChild(renderer.domElement);
    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    const gridHelper = new THREE.GridHelper(20, 20, 0x56616f, 0x28313c);
    gridHelper.name = "__grid";
    gridRef.current = gridHelper;
    scene.add(gridHelper, new THREE.HemisphereLight(0xffffff, 0x374151, 2.2));
    const key = new THREE.DirectionalLight(0xffffff, 2.4); key.position.set(5, 8, 6); scene.add(key);
    const resize = () => { const { width, height } = container.getBoundingClientRect(); renderer.setSize(Math.max(1, width), Math.max(1, height), false); camera.aspect = Math.max(1, width) / Math.max(1, height); camera.updateProjectionMatrix(); };
    const observer = new ResizeObserver(resize); observer.observe(container); resize();
    let frame = 0; const draw = () => { controls.update(); renderer.render(scene, camera); frame = requestAnimationFrame(draw); }; draw();
    const fit = (object: THREE.Object3D) => {
      const box = new THREE.Box3().setFromObject(object); const sphere = box.getBoundingSphere(new THREE.Sphere());
      const radius = Math.max(sphere.radius, .5); controls.target.copy(sphere.center); camera.near = radius / 100; camera.far = radius * 100; camera.position.copy(sphere.center).add(new THREE.Vector3(radius * 1.8, radius * 1.3, radius * 1.8)); camera.updateProjectionMatrix(); controls.update();
    };
    resetRef.current = () => modelRef.current && fit(modelRef.current);
    const url = assetMediaUrl(media.id, "original");
    const ext = media.originalName.split(".").pop()?.toLowerCase();
    const loaded = (object: THREE.Object3D) => { modelRef.current = object; scene.add(object); fit(object); };
    const failed = (reason: unknown) => setError(`无法加载 3D 预览：${String(reason)}`);
    if (ext === "glb" || ext === "gltf") new GLTFLoader().load(url, value => loaded(value.scene), undefined, failed);
    else if (ext === "fbx") new FBXLoader().load(url, loaded, undefined, failed);
    else if (ext === "obj") new OBJLoader().load(url, loaded, undefined, failed);
    else if (ext === "stl") new STLLoader().load(url, geometry => { const material = new THREE.MeshStandardMaterial({ color: 0xc5ccd6, roughness: .7 }); loaded(new THREE.Mesh(geometry, material)); }, undefined, failed);
    else setError("不支持的 3D 格式");
    return () => { cancelAnimationFrame(frame); observer.disconnect(); controls.dispose(); renderer.dispose(); renderer.domElement.remove(); scene.traverse(object => { if (object instanceof THREE.Mesh) { object.geometry?.dispose(); const materials = Array.isArray(object.material) ? object.material : [object.material]; materials.forEach(material => material.dispose()); } }); };
  }, [media.id, media.originalName]);

  useEffect(() => { modelRef.current?.traverse(object => { if (object instanceof THREE.Mesh) { const materials = Array.isArray(object.material) ? object.material : [object.material]; materials.forEach(material => { if ("wireframe" in material) (material as THREE.MeshStandardMaterial).wireframe = wireframe; }); } }); }, [wireframe]);
  useEffect(() => { if (gridRef.current) gridRef.current.visible = grid; }, [grid]);

  return <div className="model-viewer-shell"><div ref={host} className="model-viewer-canvas" />{error && <div className="model-viewer-error">{error}</div>}<div className="model-viewer-tools"><button onClick={() => resetRef.current()}>适应窗口</button><button className={grid ? "active" : ""} onClick={() => setGrid(value => !value)}>网格</button><button className={wireframe ? "active" : ""} onClick={() => setWireframe(value => !value)}>线框</button></div></div>;
}
