import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash,randomUUID} from 'node:crypto';
function contained(root,relative) {
  const resolved=path.resolve(root,relative);
  assert(relative && !path.isAbsolute(relative) && !relative.split(/[\\/]/).includes('..') && resolved.startsWith(path.resolve(root)+path.sep),'unsafe tool path');
  for(let current=resolved;current!==path.dirname(current);current=path.dirname(current)) {
    if(fs.existsSync(current))assert(!fs.lstatSync(current).isSymbolicLink(),'linked tool path rejected');
  }
  return resolved;
}
export function verifyFile(root,entry) {
  assert(/^[a-f0-9]{64}$/.test(entry.sha256),'invalid digest');
  const file=contained(root,entry.path);
  assert(fs.statSync(file).isFile(),'tool is not a file');
  const input=fs.openSync(file,'r'),buffer=Buffer.alloc(65536),hash=createHash('sha256');
  try {let count;while((count=fs.readSync(input,buffer))>0)hash.update(buffer.subarray(0,count));} finally {fs.closeSync(input);}
  assert.equal(hash.digest('hex'),entry.sha256,`digest mismatch: ${entry.path}`);
  return true;
}
if(process.argv.includes('--self-test')) {
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'stackdcc-tool-test-'));
  try {
    fs.writeFileSync(path.join(root,'fixture'),'abc');
    const file={path:'fixture',sha256:'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'};
    assert.equal(verifyFile(root,file),true);
    fs.writeFileSync(path.join(root,'fixture'),'tampered');
    assert.throws(()=>verifyFile(root,file));
    assert.throws(()=>verifyFile(root,{...file,path:'../outside'}));
  } finally {
    if(path.dirname(path.resolve(root))===path.resolve(os.tmpdir())&&path.basename(root).startsWith('stackdcc-tool-test-'))fs.rmSync(root,{recursive:true,force:true});
  }
  process.stdout.write('learning tool integrity self-check: passed\n');
} else {
  const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../src-tauri/resources');
  const manifest=JSON.parse(fs.readFileSync(path.join(root,'learning-components.json'),'utf8'));
  assert.equal(manifest.version,1);
  const sourceIndex=process.argv.indexOf('--install-from');
  const source=sourceIndex>=0?process.argv[sourceIndex+1]:null;
  assert(process.argv.includes('--verify') || source,'use --verify or --install-from <verified resource root>');
  for(const entry of manifest.files) {
    const destination=contained(root,entry.path);
    if(source&&!fs.existsSync(destination)) {
      verifyFile(source,{...entry,path:entry.sourcePath});
      fs.mkdirSync(path.dirname(destination),{recursive:true});
      const space=fs.statfsSync(path.dirname(destination),{bigint:true});
      assert(space.bavail*space.bsize>BigInt(fs.statSync(contained(source,entry.sourcePath)).size)+5n*1024n*1024n,'insufficient disk space');
      const stage=destination+'.'+randomUUID()+'.tmp';
      try {
        fs.copyFileSync(contained(source,entry.sourcePath),stage,fs.constants.COPYFILE_EXCL);
        verifyFile(root,{...entry,path:path.relative(root,stage)});
        // Same-directory hard link publishes atomically and fails rather than replacing any file.
        fs.linkSync(stage,destination);
      } finally {if(fs.existsSync(stage))fs.unlinkSync(stage);}
    }
    verifyFile(root,entry);
    process.stdout.write(`verified ${entry.path}\n`);
  }
}
