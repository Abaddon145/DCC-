// M0 only: reads fixed tool/source locations; writes generated fixtures under release-artifacts.
import {createHash, randomUUID} from 'node:crypto';
import {createReadStream} from 'node:fs';
import * as fs from 'node:fs/promises';
import {existsSync} from 'node:fs';
import {dirname, join, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import assert from 'node:assert/strict';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const main = 'C:/Users/14582/Documents/ChatGPT/视频下载软件';
const linked = 'C:/Users/14582/.codex/worktrees/content-updates/视频下载软件';
const targetTools = join(repo, 'src-tauri/resources/media-tools');
const sourceTools = join(linked, 'src-tauri/resources/tools');
await fs.mkdir(join(repo, 'release-artifacts/yingliu-merge'), {recursive:true});
const output = await fs.mkdtemp(join(repo, 'release-artifacts/yingliu-merge/m0-'));
const result = {synthetic:true, productIntegration:false, output, checks:[], inventories:{}, errors:[]};
async function hash(path) {
  const h = createHash('sha256');
  for await (const chunk of createReadStream(path)) h.update(chunk);
  return h.digest('hex');
}
function command(exe, args, allowed=[0]) {
  const p = spawnSync(exe, args, {encoding:'utf8',windowsHide:true,timeout:60000,maxBuffer:2*1024*1024});
  const record = {exe,args,exit:p.status,stdout:p.stdout||'',stderr:p.stderr||'',error:p.error?.message};
  result.checks.push(record);
  assert.ok(!p.error && allowed.includes(p.status), `command failed: ${exe} (${p.status}): ${record.stderr.slice(-500)}`);
  return record;
}
try {
  const sourceManifest = JSON.parse(await fs.readFile(join(sourceTools,'binaries.json'),'utf8'));
  for (const [label,root] of [['sourceLinked',sourceTools],['sourceMain',join(main,'src-tauri/resources/tools')],['target',targetTools]]) {
    const rows = [];
    const names = label==='sourceLinked' ? Object.keys(sourceManifest) : ['ffmpeg.exe','ffprobe.exe','yt-dlp.exe','deno.exe','mpv.exe','whisper-cli.exe',...(label==='target'?(await fs.readdir(root)).filter(n=>n.endsWith('.dll')):[])];
    for (const name of names) {
      const path = join(root,name), present = existsSync(path);
      const digest = present ? await hash(path) : null;
      const expected = label.startsWith('source') ? sourceManifest[name]?.sha256 : ['yt-dlp.exe','deno.exe'].includes(name) ? sourceManifest[name]?.sha256 : undefined;
      const row = {name,path,present,bytes:present?(await fs.stat(path)).size:0,sha256:digest,expected,match:expected&&present?digest===expected:null};
      if (row.match===false) throw new Error(`manifest mismatch: ${path}`);
      if (label==='sourceLinked') assert.ok(present, `missing fixed source tool: ${name}`);
      rows.push(row);
    }
    result.inventories[label]=rows;
  }
  const licensePaths = [
    join(targetTools,'FFMPEG-LICENSE.txt'),join(repo,'src-tauri/resources/THIRD_PARTY_NOTICES.md'),
    ...['mpv-Copyright.txt','mpv-GPL-2.txt','mpv-LGPL-2.1.txt','whisper-MIT.txt','GPL-3.txt'].map(n=>join(linked,'src-tauri/resources/licenses',n)),
    join(sourceTools,'mpv/fonts.conf')
  ];
  result.inventories.licenses = await Promise.all(licensePaths.map(async path=>({path,sha256:await hash(path),bytes:(await fs.stat(path)).size})));
  const backend = ['learning/native_player.rs','learning/native_window.rs','learning/progress.rs','learning/player.rs','learning/models.rs','learning/repository.rs','learning/capture.rs','learning/cache.rs','learning/commands.rs','learning/service.rs','learning/db.rs','native.rs',
    ...['analysis','models','references','repository','summary','jobs','section_reference_tests','template_tests','materials','subtitles','transcription','whisper_models','qa','service'].map(n=>`ai/${n}.rs`)];
  const frontend = ['learning/LearningPlayer.tsx','learning/NativeLearningSurface.tsx','learning/nativeBindings.ts','learning/playerInteraction.ts','learning/playbackHandle.ts','learning/VideoNotes.tsx',
    'ai/VideoSummary.tsx','ai/SectionValidation.tsx','ai/Transcript.tsx','ai/VideoAsk.tsx'];
  const paths = [...backend.map(p=>`src-tauri/src/${p}`),...frontend.map(p=>`src/features/${p}`),'src/types/ai.ts'];
  result.inventories.sourceFiles = await Promise.all(paths.map(async path=>({path,sha256:await hash(join(main,path))})));
  const ffmpeg=join(targetTools,'ffmpeg.exe'), ffprobe=join(targetTools,'ffprobe.exe');
  result.versions = {};
  for (const [name,exe,args] of [
    ['targetFFmpeg',ffmpeg,['-version']],['targetFFprobe',ffprobe,['-version']],
    ['sourceMpv',join(sourceTools,'mpv.exe'),['--no-config','--version']],
    ['sourceWhisper',join(sourceTools,'whisper-cli.exe'),['--help']]
  ]) result.versions[name]=command(exe,args).stdout + result.checks.at(-1).stderr;
  const config = command(ffmpeg,['-hide_banner','-buildconf']);
  assert.ok(!`${config.stdout}${config.stderr}`.includes('--enable-gpl'), 'target unexpectedly enables GPL');
  assert.ok(`${config.stdout}${config.stderr}`.includes('--enable-shared'));
  const baseArgs=['-nostdin','-hide_banner','-loglevel','error','-n'];
  const h264=join(output,'fixture-h264.mp4'), hevc=join(output,'fixture-hevc.mp4');
  for (const [file,encoder,extra] of [[h264,'libx264',[]],[hevc,'libx265',['-x265-params','pools=1:frame-threads=1:log-level=error']]]) {
    command(join(sourceTools,'ffmpeg.exe'),[...baseArgs,'-f','lavfi','-i','testsrc2=size=128x72:rate=10','-f','lavfi','-i','sine=frequency=440:sample_rate=48000','-map','0:v','-map','1:a','-map','1:a','-metadata:s:a:0','language=eng','-metadata:s:a:1','language=zho','-t','1','-c:v',encoder,'-threads','1',...extra,'-pix_fmt','yuv420p','-c:a','aac',file]);
  }
  const before = {h264:await hash(h264),hevc:await hash(hevc)};
  for (const file of [h264,hevc]) {
    const info=JSON.parse(command(ffprobe,['-v','error','-show_streams','-show_format','-of','json',file]).stdout);
    assert.ok(info.streams.some(s=>s.codec_type==='video'&&s.width===128&&s.height===72));
    assert.equal(info.streams.filter(s=>s.codec_type==='audio').length,2);
    command(ffmpeg,[...baseArgs,'-ss','0.3','-i',file,'-frames:v','1','-vf','scale=160:-2',`${file}.png`]);
    command(ffmpeg,[...baseArgs,'-i',file,'-c:v','libvpx-vp9','-threads','1','-deadline','realtime','-c:a','libopus',`${file}.webm`]);
  }
  for (const [name,codec] of [['fixture-mpeg4.avi','mpeg4'],['fixture-wmv2.wmv','wmv2']]) {
    const file=join(output,name);
    command(join(sourceTools,'ffmpeg.exe'),[...baseArgs,'-f','lavfi','-i','testsrc2=size=128x72:rate=10','-t','1','-c:v',codec,'-threads','1',file]);
    const info=JSON.parse(command(ffprobe,['-v','error','-show_streams','-of','json',file]).stdout);
    assert.equal(info.streams[0].codec_name,codec);
    command(ffmpeg,[...baseArgs,'-i',file,'-frames:v','1',`${file}.png`]);
  }
  const wav=join(output,'transcription.wav');
  command(ffmpeg,[...baseArgs,'-i',h264,'-vn','-ac','1','-ar','16000','-c:a','pcm_s16le',wav]);
  const audio=JSON.parse(command(ffprobe,['-v','error','-show_streams','-of','json',wav]).stdout).streams[0];
  assert.equal(audio.codec_name,'pcm_s16le'); assert.equal(audio.sample_rate,'16000'); assert.equal(audio.channels,1);
  command(ffmpeg,[...baseArgs,'-i',wav,'-filter_complex','showwavespic=s=320x80:colors=white','-frames:v','1',join(output,'waveform.png')]);
  assert.deepEqual({h264:await hash(h264),hevc:await hash(hevc)},before);
  result.media = {before,afterUnchanged:true,toolCompatibility:'H264/HEVC decode, thumbnails, VP9+Opus proxy, PCM16 16k mono, waveform'};
  await fs.writeFile(join(output,'chinese.srt'),'1\r\n00:00:00,100 --> 00:00:00,350\r\n第一段完整字幕。\r\n\r\n2\r\n00:00:00,600 --> 00:00:00,900\r\n第二段：保留不连续范围。\r\n');

  const vault=join(output,'source-vault'), archived=join(output,'archived-vault'), restored=join(output,'restored-vault');
  await fs.mkdir(join(vault,'Attachments'),{recursive:true});
  const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a9h0AAAAASUVORK5CYII=','base64');
  const md=Buffer.concat([Buffer.from([0xef,0xbb,0xbf]),Buffer.from('---\r\ntags: [UE5]\r\n---\r\n# 合成学习\r\n[[另一篇|别名]]\r\n![[Attachments/frame.png]]\r\n- 项目\r\n  - 子项\r\n\r\n```cpp\r\nint value = 1;\r\n```\r\n\r\n![[missing.png]]\r\n')]);
  const unit=Buffer.from('合成教程段落，保留原文。\r\n\r\n');
  const prefix=Buffer.from('# 长笔记\r\n');
  const n=Math.floor((614400-prefix.length)/unit.length);
  const long=Buffer.concat([prefix,...Array(n).fill(unit),Buffer.alloc(614400-prefix.length-n*unit.length,0x20)]);
  await fs.writeFile(join(vault,'note.md'),md); await fs.writeFile(join(vault,'same-content-other-note.md'),md);
  await fs.writeFile(join(vault,'long.md'),long); await fs.writeFile(join(vault,'Attachments/frame.png'),png);
  await fs.cp(vault,archived,{recursive:true,errorOnExist:true,force:false}); await fs.cp(archived,restored,{recursive:true,errorOnExist:true,force:false});
  const fixtureFiles=['note.md','same-content-other-note.md','long.md','Attachments/frame.png'];
  result.noteFixtures=[];
  for (const relative of fixtureFiles) {
    const expected=await hash(join(vault,relative));
    assert.equal(await hash(join(archived,relative)),expected); assert.equal(await hash(join(restored,relative)),expected);
    result.noteFixtures.push({relative,bytes:(await fs.stat(join(vault,relative))).size,sha256:expected});
  }
  assert.equal(long.length,614400); assert.ok(!existsSync(join(vault,'missing.png')));
  const imageInfo=JSON.parse(command(ffprobe,['-v','error','-show_streams','-of','json',join(vault,'Attachments/frame.png')]).stdout).streams[0];
  assert.equal(imageInfo.width,1); assert.equal(imageInfo.height,1);
  command(ffmpeg,[...baseArgs,'-i',join(vault,'Attachments/frame.png'),'-f','null','-']);
  const python='C:/Users/14582/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe';
  const sqlCheck=`import sqlite3, pathlib, json, hashlib, sys
p=pathlib.Path(sys.argv[1]); src=p/'wal-source.sqlite'; dst=p/'wal-snapshot.sqlite'
w=sqlite3.connect(src); w.execute('pragma journal_mode=wal'); w.execute('create table fixture(id integer primary key, value text)'); w.execute("insert into fixture(value) values('before')"); w.commit(); w.execute("insert into fixture(value) values('committed-in-wal')"); w.commit()
assert pathlib.Path(str(src)+'-wal').exists()
before=hashlib.sha256(src.read_bytes()).hexdigest(); ro=sqlite3.connect(src.as_uri()+'?mode=ro',uri=True); snap=sqlite3.connect(dst); ro.backup(snap)
assert snap.execute('select count(*) from fixture').fetchone()[0]==2
assert snap.execute('pragma integrity_check').fetchone()[0]=='ok'
assert hashlib.sha256(src.read_bytes()).hexdigest()==before
try:
 ro.execute("insert into fixture(value) values('forbidden')")
 raise AssertionError('read-only connection unexpectedly writable')
except sqlite3.OperationalError: pass
ro.close(); snap.close(); w.close(); print(json.dumps({'rows':2,'readonly':True,'walIncluded':True,'actualSourceSchema':False}))`;
  result.sqlite=JSON.parse(command(python,['-c',sqlCheck,output]).stdout);
  result.identityFixture={testCaseOnly:true,notAnImportFormat:true,sourceProfileId:randomUUID(),
    notes:[{sourceKey:'vault-fixture/note.md',sha256:await hash(join(vault,'note.md'))},{sourceKey:'vault-fixture/same-content-other-note.md',sha256:await hash(join(vault,'same-content-other-note.md'))}],
    media:{sha256:before.h264,seriesMemberships:['series-a','series-b']},missingAttachmentExpected:1};
  await fs.writeFile(join(output,'identity-conflicts.json'),JSON.stringify(result.identityFixture,null,2));
  result.status='passed';
} catch(e) { result.status='failed'; result.errors.push(String(e.stack||e)); process.exitCode=1; }
await fs.writeFile(join(output,'results.json'),JSON.stringify(result,null,2));
console.log(JSON.stringify({status:result.status,output,checks:result.checks.length,errors:result.errors},null,2));
