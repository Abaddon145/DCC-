import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const repo=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
const parent=path.join(repo,'release-artifacts/yingliu-merge');
fs.mkdirSync(parent,{recursive:true});
const output=fs.mkdtempSync(path.join(parent,'native-host-'));
const binary=path.join(repo,'src-tauri/target/debug/dcc-asset-library.exe');
if(!fs.existsSync(binary)) throw new Error('Build the debug DCC target first (cargo build --locked).');
const result=spawnSync(binary,['--learning-native-qa',output],{
  cwd:repo,encoding:'utf8',windowsHide:true,timeout:90000,maxBuffer:1024*1024,
});
fs.writeFileSync(path.join(output,'process.txt'),`${result.stdout??''}\n${result.stderr??''}\nstatus=${result.status}; error=${result.error??''}`);
const verification=result.status===0?spawnSync(binary,['--learning-native-qa-verify',output],{cwd:repo,encoding:'utf8',windowsHide:true,timeout:30000,maxBuffer:1024*1024}):null;
if(verification)fs.writeFileSync(path.join(output,'verification.txt'),`${verification.stdout??''}\n${verification.stderr??''}\nstatus=${verification.status}; error=${verification.error??''}`);
const report=JSON.parse(fs.readFileSync(path.join(output,'results.json'),'utf8'));
if(report.native===true){
  for(const pid of report.result?.Ok?.pids??[]){
    let alive=true;try{process.kill(pid,0);}catch(error){if(error.code==='ESRCH')alive=false;else throw error;}
    if(alive)throw new Error(`Unreaped native process: ${pid}`);
  }
}
const verified=result.status===0&&verification?.status===0&&report.native===true&&report.postExitVerified===true;
console.log(JSON.stringify({output,...report,native:verified},null,2));
if(!verified) process.exitCode=1;
