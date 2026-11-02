//! Private transport only; frontend commands must use the typed PlaybackAction boundary.
use crate::runtime::process::OwnedProcess;
use serde_json::Value;
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};
type Request = (Vec<Value>, mpsc::Sender<Result<Value, String>>);
pub(super) struct NativeIpc {
    process: Arc<OwnedProcess>,
    commands: mpsc::SyncSender<Request>,
}
pub(super) fn read_line(reader: &mut BufReader<File>) -> Result<String, String> {
    let mut line = String::new();
    let count = reader
        .take(1024 * 1024 + 1)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    if count == 0 || count > 1024 * 1024 || !line.ends_with('\n') {
        return Err("播放内核已断开或响应过大".into());
    }
    Ok(line)
}
pub(super) fn open_pipe(
    process: &OwnedProcess,
    pipe: &str,
    stop: &dyn Fn() -> bool,
) -> Result<File, String> {
    if !pipe.starts_with(r"\\.\pipe\stackdcc-") || pipe.chars().any(char::is_control) {
        return Err("播放会话通道无效".into());
    }
    let start = Instant::now();
    loop {
        if stop() || process.exited() || start.elapsed() > Duration::from_secs(3) {
            let _ = process.terminate();
            return Err("播放内核连接已取消、退出或超时".into());
        }
        if let Ok(file) = OpenOptions::new().read(true).write(true).open(pipe) {
            return Ok(file);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
impl NativeIpc {
    pub fn connect(
        process: Arc<OwnedProcess>,
        pipe: &str,
        stop: &dyn Fn() -> bool,
    ) -> Result<Self, String> {
        let file = open_pipe(&process, pipe, stop)?;
        let (commands, receiver) = mpsc::sync_channel::<Request>(64);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(file);
            for (id, (command, reply)) in receiver.into_iter().enumerate() {
                let result = (|| {
                    writeln!(
                        reader.get_mut(),
                        "{}",
                        serde_json::json!({"command":command,"request_id":id as u64})
                    )
                    .map_err(|e| e.to_string())?;
                    loop {
                        if let Some(value) = parse_reply(&read_line(&mut reader)?, id as u64)? {
                            return Ok(value);
                        }
                    }
                })();
                let _ = reply.send(result);
            }
        });
        Ok(Self { process, commands })
    }
    pub fn request(&self, command: Vec<Value>) -> Result<Value, String> {
        let (send, receive) = mpsc::channel();
        self.commands
            .try_send((command, send))
            .map_err(|_| "播放操作过多或会话已关闭")?;
        match receive.recv_timeout(Duration::from_secs(3)) {
            Ok(result) => result,
            Err(_) => {
                let _ = self.process.terminate();
                Err("播放内核响应超时，进程已安全停止".into())
            }
        }
    }
}
impl Drop for NativeIpc {
    fn drop(&mut self) {
        let _ = self.process.terminate();
    }
}
fn parse_reply(line: &str, expected: u64) -> Result<Option<Value>, String> {
    let value: Value = serde_json::from_str(line).map_err(|_| "播放内核返回无效数据")?;
    if value.get("request_id").and_then(Value::as_u64) != Some(expected) {
        return Ok(None);
    }
    if value["error"] != "success" {
        return Err(format!(
            "播放操作失败：{}",
            value["error"].as_str().unwrap_or("未知错误")
        ));
    }
    Ok(Some(value.get("data").cloned().unwrap_or(Value::Null)))
}
#[cfg(test)]
mod learning_native_ipc_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn learning_native_ipc_correlates_reply_and_ignores_events() {
        assert!(parse_reply(r#"{"event":"property-change","data":3}"#, 7)
            .unwrap()
            .is_none());
        assert!(
            parse_reply(r#"{"request_id":8,"data":3,"error":"success"}"#, 7)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            parse_reply(r#"{"request_id":7,"data":3,"error":"success"}"#, 7).unwrap(),
            Some(json!(3))
        );
        assert!(parse_reply(r#"{"request_id":7,"error":"failure"}"#, 7).is_err());
    }
    #[test]
    fn learning_native_ipc_uses_verified_mpv_component_and_reaps_it() {
        // mpv.exe is gitignored (~121MB). CI does not have it; a local verified tree
        // restores it with scripts/prepare-learning-tools.mjs --install-from.
        let exe = match crate::runtime::tools::require(None, "mpv") {
            Ok(path) => path,
            Err(error) if error.starts_with("工具缺失：mpv") => {
                eprintln!(
                    "跳过：{error}。src-tauri/resources/media-tools/mpv.exe 不入库，Actions 上没有这份二进制。本机用 scripts/prepare-learning-tools.mjs --install-from <verified resource root> 恢复后，本测试会校验摘要、版本并回收进程。"
                );
                return;
            }
            Err(error) => panic!("已放置的 mpv 未通过固定摘要校验：{error}"),
        };
        let pipe = format!(r"\\.\pipe\stackdcc-component-{}", uuid::Uuid::new_v4());
        let mut args = [
            "--no-config",
            "--load-scripts=no",
            "--load-auto-profiles=no",
            "--input-default-bindings=no",
            "--ytdl=no",
            "--access-references=no",
            "--terminal=no",
            "--idle=yes",
            "--vo=null",
            "--ao=null",
        ]
        .map(str::to_owned)
        .to_vec();
        args.push(format!("--input-ipc-server={pipe}"));
        let (process, _out, _err) = crate::runtime::process::spawn_owned(&exe, &args).unwrap();
        let process = Arc::new(process);
        let ipc = NativeIpc::connect(process.clone(), &pipe, &|| false).unwrap();
        let version = ipc
            .request(vec![json!("get_property"), json!("mpv-version")])
            .unwrap();
        assert!(version.as_str().unwrap().contains("492bae076"));
        drop(ipc);
        assert!(process.exited(), "IPC owner must close its process");
    }
}
