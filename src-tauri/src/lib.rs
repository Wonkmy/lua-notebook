use mlua::{Lua, Value};
use once_cell::sync::Lazy;
use std::sync::{Arc, Mutex};

struct LuaKernel {
    lua: Lua,
    output: Arc<Mutex<String>>,
}

impl LuaKernel {
    fn new() -> Self {
        let lua = Lua::new();
        let output = Arc::new(Mutex::new(String::new()));

        let output_clone = output.clone();

        let print_func = lua.create_function(move |_, args: mlua::Variadic<Value>| {
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
        }).unwrap();

        lua.globals().set("print", print_func).unwrap();

        Self { lua, output }
    }

    fn run(&mut self, code: String) -> String {
        self.output.lock().unwrap().clear();

        match self.lua.load(&code).exec() {
            Ok(_) => {
                let output = self.output.lock().unwrap();

                if output.is_empty() {
                    "执行完成".to_string()
                } else {
                    output.clone()
                }
            }
            Err(e) => format!("Lua错误：{}", e),
        }
    }
}

static LUA_KERNEL: Lazy<Mutex<LuaKernel>> = Lazy::new(|| Mutex::new(LuaKernel::new()));

#[tauri::command]
fn run_lua(code: String) -> String {
    let mut kernel = LUA_KERNEL.lock().unwrap();
    kernel.run(code)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![run_lua])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}