//! Lua 5.4 execution for SjASMPlus. The interpreter lives for one assembly;
//! scoped callbacks borrow its current pass only while a chunk is running.

use std::{cell::Cell, rc::Rc};

use mlua::chunk::ChunkMode;
use mlua::{Function, HookTriggers, Lua, LuaOptions, MultiValue, StdLib, Value, VmState};

const MEMORY_LIMIT: usize = 32 * 1024 * 1024;
const INSTRUCTION_LIMIT: u64 = 10_000_000;
const HOOK_INTERVAL: u32 = 100;
const DISPATCH: &str = "asm198x.sj.dispatch";

#[derive(Clone)]
pub(super) struct Runtime {
    lua: Lua,
    instructions: Rc<Cell<u64>>,
    host_bytes: Rc<Cell<usize>>,
}

impl Runtime {
    pub(super) fn new() -> mlua::Result<Self> {
        let lua = Lua::new_with(
            StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8 | StdLib::COROUTINE,
            LuaOptions::default(),
        )?;
        lua.set_memory_limit(MEMORY_LIMIT)?;
        let instructions = Rc::new(Cell::new(0u64));
        let count = instructions.clone();
        lua.set_global_hook(
            HookTriggers::new().every_nth_instruction(HOOK_INTERVAL),
            move |_, _| {
                count.set(count.get().saturating_add(u64::from(HOOK_INTERVAL)));
                if count.get() >= INSTRUCTION_LIMIT {
                    return Err(mlua::Error::runtime(
                        "Lua instruction budget exhausted (10000000 instructions)",
                    ));
                }
                Ok(VmState::Continue)
            },
        )?;
        // Base Lua loads these even when IO and OS libraries are excluded.
        for name in ["dofile", "loadfile"] {
            lua.globals().set(name, Value::Nil)?;
        }
        // Keep both standard forms (string and reader function), but never
        // let either load bytecode. Capture the original loader privately.
        lua.load(
            r#"
            local load_text, find, globals = load, string.find, _G
            load = function(source, name, mode, env)
                if mode ~= nil and not find(mode, 't', 1, true) then
                    return nil, 'binary Lua chunks are disabled'
                end
                return load_text(source, name, 't', env or globals)
            end
        "#,
        )
        .exec()?;
        // Coroutines created by Rust inherit the global instruction hook.
        let coroutine: mlua::Table = lua.globals().get("coroutine")?;
        coroutine.set(
            "create",
            lua.create_function(|lua, function: Function| lua.create_thread(function))?,
        )?;
        lua.load("local create, resume, error, unpack, pack = coroutine.create, coroutine.resume, error, table.unpack, table.pack\ncoroutine.wrap = function(f) local co = create(f); return function(...) local r = pack(resume(co, ...)); if not r[1] then error(r[2], 2) end; return unpack(r, 2, r.n) end end\nmath.randomseed(0, 0)").exec()?;
        // Stable functions can be saved by scripts across blocks and passes.
        // Only the registry's scoped dispatch function changes between calls.
        let dispatch = lua.create_function(|lua, args: MultiValue| {
            lua.named_registry_value::<Function>(DISPATCH)?
                .call::<MultiValue>(args)
        })?;
        let factory: Function = lua.load("return function(dispatch, name) return function(...) return dispatch(name, ...) end end").eval()?;
        lua.globals().set(
            "print",
            factory.call::<Function>((dispatch.clone(), "print"))?,
        )?;
        lua.load("local seed = math.randomseed; math.randomseed = function(x, y) if x == nil then return seed(0, 0) end; return seed(x, y or 0) end").exec()?;
        let sj = lua.create_table()?;
        for name in [
            "calc",
            "parse_line",
            "parse_code",
            "get_label",
            "insert_label",
            "get_define",
            "insert_define",
            "add_byte",
            "add_word",
            "get_byte",
            "get_word",
            "get_modules",
            "get_device",
            "set_device",
            "set_page",
            "set_slot",
            "get_page_at",
            "file_exists",
            "error",
            "warning",
            "exit",
            "shellexec",
        ] {
            sj.set(name, factory.call::<Function>((dispatch.clone(), name))?)?;
        }
        let meta = lua.create_table()?;
        let getter: Function = lua
            .load(
                "return function(dispatch) return function(_, name) return dispatch(name) end end",
            )
            .eval()?;
        meta.set("__index", getter.call::<Function>(dispatch.clone())?)?;
        meta.set(
            "__newindex",
            lua.create_function(
                |_, (_table, name, _value): (mlua::Table, String, Value)| -> mlua::Result<()> {
                    Err(mlua::Error::runtime(format!("sj.{name} is read-only")))
                },
            )?,
        )?;
        sj.set_metatable(Some(meta))?;
        for (alias, name) in [("_c", "calc"), ("_pl", "parse_line"), ("_pc", "parse_code")] {
            lua.globals().set(alias, sj.get::<Function>(name)?)?;
        }
        lua.globals().set("sj", sj)?;
        let zx = lua.create_table()?;
        for name in ["trdimage_create", "trdimage_add_file", "save_snapshot_sna"] {
            zx.set(
                name,
                lua.create_function(move |_, _: MultiValue| -> mlua::Result<()> {
                    Err(mlua::Error::runtime(format!(
                        "zx.{name} is disabled in the Lua sandbox"
                    )))
                })?,
            )?;
        }
        lua.globals().set("zx", zx)?;
        Ok(Self {
            lua,
            instructions,
            host_bytes: Rc::new(Cell::new(0)),
        })
    }

    /// Account for generated assembler work, which does not execute Lua VM
    /// instructions and is not allocated by Lua's memory allocator.
    pub(super) fn charge_work(&self, units: u64) -> mlua::Result<()> {
        self.instructions
            .set(self.instructions.get().saturating_add(units));
        if self.instructions.get() >= INSTRUCTION_LIMIT {
            return Err(mlua::Error::runtime(
                "Lua instruction budget exhausted (10000000 instructions, including generated assembler work)",
            ));
        }
        Ok(())
    }

    pub(super) fn charge_host_bytes(&self, bytes: usize) -> mlua::Result<()> {
        self.host_bytes
            .set(self.host_bytes.get().saturating_add(bytes));
        if self.host_bytes.get() > MEMORY_LIMIT {
            return Err(mlua::Error::runtime(
                "Lua-generated assembler data exceeds the 32 MiB host allocation budget",
            ));
        }
        Ok(())
    }

    pub(super) fn execute(
        &self,
        source: &str,
        name: &str,
        mut host: impl FnMut(&Lua, String, MultiValue) -> mlua::Result<MultiValue>,
    ) -> mlua::Result<()> {
        let previous = self.lua.named_registry_value::<Value>(DISPATCH)?;
        let result = self.lua.scope(|scope| {
            let function = scope.create_function_mut(|lua, mut args: MultiValue| {
                let method = match args.pop_front() {
                    Some(Value::String(method)) => method.to_str()?.to_string(),
                    _ => return Err(mlua::Error::runtime("missing Lua host operation")),
                };
                host(lua, method, args)
            })?;
            self.lua.set_named_registry_value(DISPATCH, function)?;
            self.lua
                .load(source)
                .set_name(name)
                .set_mode(ChunkMode::Text)
                .exec()
        });
        self.lua.set_named_registry_value(DISPATCH, previous)?;
        if self.instructions.get() >= INSTRUCTION_LIMIT {
            return Err(mlua::Error::runtime(
                "Lua instruction budget exhausted (10000000 instructions)",
            ));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(runtime: &Runtime, source: &str) -> mlua::Result<()> {
        runtime.execute(source, "test", |_, _, _| Ok(MultiValue::new()))
    }

    #[test]
    fn sandbox_has_no_files_processes_or_binary_loader() {
        let runtime = Runtime::new().expect("runtime");
        run(&runtime, "assert(io == nil and os == nil and package == nil and debug == nil and require == nil and loadfile == nil and dofile == nil); assert(load(string.dump(function() end)) == nil); assert(load('return 42')() == 42)").expect("closed sandbox");
        run(&runtime, "local parts = {'return ', 'answer'}; local i = 0; local f = assert(load(function() i = i + 1; return parts[i] end, 'reader', 'bt', {answer = 42})); assert(f() == 42)").expect("text reader");
        run(&runtime, "local once = false; assert(load(function() if once then return nil end; once = true; return string.dump(function() end) end) == nil)").expect("binary reader refused");
    }

    #[test]
    fn callbacks_and_globals_survive_between_scopes() {
        let runtime = Runtime::new().expect("runtime");
        run(&runtime, "saved = sj.add_byte; counter = 7").expect("first block");
        let mut values = Vec::new();
        runtime
            .execute("saved(counter + 1)", "test", |_, method, args| {
                assert_eq!(method, "add_byte");
                values.push(args.front().cloned());
                Ok(MultiValue::new())
            })
            .expect("second block");
        assert_eq!(values, vec![Some(Value::Integer(8))]);
    }

    #[test]
    fn instruction_budget_covers_coroutines_and_caught_errors() {
        for source in [
            "while true do end",
            "coroutine.resume(coroutine.create(function() while true do end end))",
            "coroutine.wrap(function() while true do end end)()",
            "pcall(function() while true do end end)",
        ] {
            let runtime = Runtime::new().expect("runtime");
            let error = run(&runtime, source).expect_err("bounded");
            assert!(error.to_string().contains("instruction budget"), "{error}");
        }
    }

    #[test]
    fn memory_is_bounded() {
        let runtime = Runtime::new().expect("runtime");
        assert!(run(&runtime, "local s = string.rep('x', 64 * 1024 * 1024)").is_err());
    }
}
