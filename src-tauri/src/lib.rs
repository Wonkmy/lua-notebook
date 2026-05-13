use mlua::{HookTriggers, Lua, Value, VmState};
use once_cell::sync::Lazy;
use std::{
    env,
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
};

type LuaTask = (String, mpsc::Sender<String>);

struct BuiltinKernel {
    sender: mpsc::Sender<LuaTask>,
    stop_flag: Arc<AtomicBool>,
    builtins: Arc<Mutex<Vec<String>>>,
}

impl BuiltinKernel {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel::<LuaTask>();

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        let builtins = Arc::new(Mutex::new(Vec::<String>::new()));
        let builtins_clone = builtins.clone();

        thread::spawn(move || {
            let lua = Lua::new();

            // 记录 Lua 初始全局变量
            {
                let globals = lua.globals();

                let mut list = builtins_clone.lock().unwrap();

                for pair in globals.pairs::<Value, Value>() {
                    if let Ok((k, _)) = pair {
                        if let Value::String(s) = k {
                            list.push(s.to_string_lossy());
                        }
                    }
                }
            }

            let output = Arc::new(Mutex::new(String::new()));

            // Stop 检查
            lua.set_hook(
                HookTriggers {
                    every_nth_instruction: Some(1000),
                    ..Default::default()
                },
                move |_lua, _debug| {
                    if stop_flag_clone.load(Ordering::Relaxed) {
                        return Err(mlua::Error::RuntimeError(
                            "Lua执行已停止".to_string(),
                        ));
                    }

                    Ok(VmState::Continue)
                },
            )
            .unwrap();

            // 重写 print
            {
                let output_clone = output.clone();

                let print_func = lua
                    .create_function(move |_, args: mlua::Variadic<Value>| {
                        let mut texts: Vec<String> = Vec::new();

                        for value in args {
                            let text = match value {
                                Value::Nil => "nil".to_string(),
                                Value::Boolean(v) => v.to_string(),
                                Value::Integer(v) => v.to_string(),
                                Value::Number(v) => v.to_string(),
                                Value::String(v) => v.to_string_lossy(),
                                Value::Table(_) => "table".to_string(),
                                Value::Function(_) => "function".to_string(),
                                _ => "value".to_string(),
                            };

                            texts.push(text);
                        }

                        let mut output = output_clone.lock().unwrap();

                        output.push_str(&texts.join("\t"));

                        output.push('\n');

                        Ok(())
                    })
                    .unwrap();

                lua.globals().set("print", print_func).unwrap();
            }

            loop {
                let Ok((code, result_tx)) = rx.recv() else {
                    break;
                };

                output.lock().unwrap().clear();

                let result = match lua.load(&code).exec() {
                    Ok(_) => {
                        let text = output.lock().unwrap();

                        if text.is_empty() {
                            "执行完成".to_string()
                        } else {
                            text.clone()
                        }
                    }

                    Err(e) => format!("Lua错误: {}", e),
                };

                let _ = result_tx.send(result);
            }
        });

        Self {
            sender: tx,
            stop_flag,
            builtins,
        }
    }

    fn run(&self, code: String) -> String {
        self.stop_flag.store(false, Ordering::Relaxed);

        let (result_tx, result_rx) = mpsc::channel();

        if self.sender.send((code, result_tx)).is_err() {
            return "Lua Kernel 发送任务失败".to_string();
        }

        result_rx
            .recv()
            .unwrap_or_else(|_| "Lua Kernel 接收结果失败".to_string())
    }

    fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    fn globals(&self) -> Vec<(String, String)> {
        let builtins = self.builtins.lock().unwrap().clone();

        let code = r#"
            for k, v in pairs(_G) do
                if type(k) == "string" then
                    print(k .. "||" .. type(v))
                end
            end
        "#;

        let text = self.run(code.to_string());

        parse_globals_output(&text, &builtins)
    }
}

static BUILTIN_KERNEL: Lazy<Mutex<Arc<BuiltinKernel>>> =
    Lazy::new(|| Mutex::new(Arc::new(BuiltinKernel::new())));

#[tauri::command]
async fn run_lua(code: String) -> Result<String, String> {
    let kernel = {
        let guard = BUILTIN_KERNEL.lock().unwrap();
        guard.clone()
    };

    let result =
        tauri::async_runtime::spawn_blocking(move || kernel.run(code)).await;

    match result {
        Ok(v) => Ok(v),
        Err(e) => Err(format!("线程执行失败: {}", e)),
    }
}

#[tauri::command]
fn restart_lua() {
    let kernel = {
        let guard = BUILTIN_KERNEL.lock().unwrap();
        guard.clone()
    };

    kernel.stop();
}

#[tauri::command]
fn reset_lua() {
    let mut kernel = BUILTIN_KERNEL.lock().unwrap();

    *kernel = Arc::new(BuiltinKernel::new());
}

#[tauri::command]
fn get_globals() -> Vec<(String, String)> {
    let kernel = {
        let guard = BUILTIN_KERNEL.lock().unwrap();
        guard.clone()
    };

    kernel.globals()
}

// 自定义 Lua Runtime（一次性运行）
#[tauri::command]
async fn run_custom_lua(code: String) -> Result<String, String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        let Some(path) = find_lua53_runtime() else {
            return "找不到 lua-runtime/lua53.exe".to_string();
        };

        let output = Command::new(path)
            .arg("-e")
            .arg(code)
            .output();

        match output {
            Ok(result) => {
                let stdout = String::from_utf8_lossy(&result.stdout);

                let stderr = String::from_utf8_lossy(&result.stderr);

                let text = format!("{}{}", stdout, stderr);

                if text.is_empty() {
                    "执行完成".to_string()
                } else {
                    text
                }
            }

            Err(e) => format!("启动自定义 Lua 失败: {}", e),
        }
    })
    .await;

    match result {
        Ok(v) => Ok(v),
        Err(e) => Err(format!("线程执行失败: {}", e)),
    }
}

fn parse_globals_output(
    text: &str,
    builtins: &Vec<String>,
) -> Vec<(String, String)> {
    let mut list = Vec::new();

    for line in text.lines() {
        if let Some(index) = line.find("||") {
            let key = line[..index].trim().to_string();

            let value = line[index + 2..].trim().to_string();

            if !builtins.contains(&key) {
                list.push((key, value));
            }
        }
    }

    list
}

fn find_lua53_runtime() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(dir) = env::current_dir() {
        candidates.push(dir.join("lua-runtime").join("lua53.exe"));

        candidates.push(dir.join("../lua-runtime").join("lua53.exe"));

        candidates.push(dir.join("../../lua-runtime").join("lua53.exe"));
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("lua-runtime").join("lua53.exe"));

            candidates.push(dir.join("../lua-runtime").join("lua53.exe"));

            candidates.push(dir.join("../../lua-runtime").join("lua53.exe"));

            candidates.push(dir.join("../../../lua-runtime").join("lua53.exe"));
        }
    }

    for path in candidates {
        if path.exists() {
            return Some(path);
        }
    }

    None
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            run_lua,
            restart_lua,
            reset_lua,
            get_globals,
            run_custom_lua
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
