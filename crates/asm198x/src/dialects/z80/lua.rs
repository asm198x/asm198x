//! Synchronous Lua calls into the current SjASMPlus pass.

use super::*;
use mlua::{FromLuaMulti, IntoLuaMulti, Lua, MultiValue};

impl<S: Z80Syntax> SjasmEval<'_, S> {
    pub(super) fn lower_lua(
        &mut self,
        node: &Node,
        out: &mut Vec<Statement>,
    ) -> Result<bool, AsmError> {
        let line = node.span.line as usize;
        let (word, args) = split_first_word(&node.source);
        let word = word.trim_start_matches('.');
        if !matches!(word, "lua" | "LUA" | "includelua" | "INCLUDELUA") {
            return Ok(false);
        }
        if self.lua.is_none() {
            self.lua = Some(
                crate::dialects::sjasmplus_lua::Runtime::new()
                    .map_err(|e| AsmError::new(line, e.to_string()))?,
            );
        }
        let mut warn_machine_code = false;
        let (source, file, first_line) = if matches!(word, "includelua" | "INCLUDELUA") {
            if self.pass != 1 {
                return Ok(true);
            }
            let request = include_request(args, line)?;
            let mcx = self.multi.as_mut().ok_or_else(|| {
                AsmError::new(line, "INCLUDELUA needs the multi-file entry point")
            })?;
            let id = mcx
                .map
                .load(mcx.loader, &request, node.span.file, node.span.line)
                .map_err(|e| AsmError::new(line, e.to_string()))?;
            (mcx.map.contents(id).unwrap_or_default().to_string(), id, 1)
        } else {
            let mut lines = node.source.lines();
            let header = lines.next().unwrap_or_default();
            let mode = split_first_word(self.syntax.strip_comment(header)).1.trim();
            let pass = match mode {
                "" | "PASS3" | "pass3" => 3,
                "PASS1" | "pass1" => 1,
                "PASS2" | "pass2" => 2,
                "ALLPASS" | "allpass" => 0,
                _ => return Err(AsmError::new(line, format!("invalid LUA pass `{mode}`"))),
            };
            let mut body = String::new();
            let mut closed = false;
            let mut acknowledged = header.contains("luamc-ok");
            for raw in lines {
                if matches!(
                    split_first_word(raw).0.trim_start_matches('.'),
                    "endlua" | "ENDLUA"
                ) {
                    closed = true;
                    acknowledged |= raw.contains("luamc-ok");
                    break;
                }
                body.push_str(raw);
                body.push('\n');
            }
            if !closed {
                return Err(AsmError::new(line, "LUA has no matching ENDLUA"));
            }
            if pass != 0 && pass != self.pass {
                return Ok(true);
            }
            warn_machine_code = pass != 0 && !acknowledged;
            (body, node.span.file, line + 1)
        };
        let runtime = self.lua.as_ref().expect("initialised above").clone();
        let previous_file = self.current_file;
        self.current_file = file;
        let name = self
            .multi
            .as_ref()
            .and_then(|m| m.map.path(file))
            .unwrap_or("source");
        let name = format!("@{name}:{first_line}");
        // Assembler failures remain failures even when Lua catches the
        // callback exception with pcall/coroutine.resume.
        let mut host_failure = None;
        let first_statement = out.len();
        let result = runtime.execute(&source, &name, |lua, method, args| {
            let result = self.lua_call(lua, &method, args, node, out);
            if let Err(error) = &result {
                host_failure.get_or_insert_with(|| error.to_string());
            }
            result
        });
        self.current_file = previous_file;
        if let Some(error) = host_failure {
            return Err(stamp_file(
                AsmError::new(first_line, format!("[LUA] {error}")),
                file,
            ));
        }
        result.map_err(|e| stamp_file(AsmError::new(first_line, format!("[LUA] {e}")), file))?;
        if self.pass == 3
            && warn_machine_code
            && out[first_statement..].iter().any(|statement| {
                statement.op.as_ref().is_some_and(|op| {
                    crate::engine::next_pc(op, 0, self.set, self.ext, 1, line)
                        .is_ok_and(|pc| pc > 0)
                })
            })
        {
            self.lua_warnings.push(Warning {
                line,
                file,
                message: "[luamc] Lua emitted machine code outside ALLPASS; use ALLPASS or acknowledge with luamc-ok".into(),
                kind: crate::engine::WarningKind::Advisory,
            });
        }
        Ok(true)
    }

    fn lua_parse(
        &mut self,
        source: &str,
        node: &Node,
        out: &mut Vec<Statement>,
    ) -> Result<(), AsmError> {
        // The injected source is parsed and evaluated before control returns to
        // Lua, so subsequent reads see its labels, PC and emitted bytes.
        let mut program = parse_text_keyword(
            self.syntax,
            self.set,
            self.ext,
            self.current_file,
            source,
            None,
        )?;
        for n in &mut program.nodes {
            n.span.line = node.span.line;
        }
        crate::ast::evaluate(self, &program.nodes, true, out)
    }

    fn lua_call(
        &mut self,
        lua: &Lua,
        method: &str,
        args: MultiValue,
        node: &Node,
        out: &mut Vec<Statement>,
    ) -> mlua::Result<MultiValue> {
        let line = node.span.line as usize;
        let asm_error = |e: AsmError| mlua::Error::runtime(e.to_string());
        match method {
            "get_device" => self
                .lua_device(out)
                .map(|(spec, _, _)| spec.name)
                .unwrap_or_else(|| "NONE".into())
                .into_lua_multi(lua),
            "set_device" => {
                let (name, ramtop) = <(Option<String>, Option<i64>)>::from_lua_multi(args, lua)?;
                if ramtop.is_some_and(|value| value != 0) {
                    return Err(mlua::Error::runtime(
                        "sj.set_device RAMTOP is not implemented",
                    ));
                }
                let name = name.unwrap_or_else(|| "NONE".into());
                let result = self.lua_parse(&format!(" DEVICE {name}"), node, out);
                if result.is_err() {
                    self.lua_parse(" DEVICE NONE", node, out)
                        .map_err(asm_error)?;
                }
                result.is_ok().into_lua_multi(lua)
            }
            "set_slot" | "set_page" => {
                let value = i64::from_lua_multi(args, lua)?;
                let Some((spec, _, _)) = self.lua_device(out) else {
                    return false.into_lua_multi(lua);
                };
                let value = if method == "set_slot"
                    && value >= spec.slots as i64
                    && value % spec.slot_size as i64 == 0
                {
                    value / spec.slot_size as i64
                } else {
                    value
                };
                let bound = if method == "set_slot" {
                    spec.slots
                } else {
                    spec.pages
                };
                if value < 0 || value >= bound as i64 {
                    return false.into_lua_multi(lua);
                }
                let word = if method == "set_slot" { "SLOT" } else { "PAGE" };
                self.lua_parse(&format!(" {word} {value}"), node, out)
                    .map_err(asm_error)?;
                true.into_lua_multi(lua)
            }
            "get_page_at" => {
                let address = i64::from_lua_multi(args, lua)?;
                let value = self
                    .lua_device(out)
                    .filter(|_| (0..65536).contains(&address))
                    .map(|(spec, slots, _)| slots[address as usize / spec.slot_size] as i64)
                    .unwrap_or(-1);
                value.into_lua_multi(lua)
            }
            "get_byte" | "get_word" => {
                let address = i64::from_lua_multi(args, lua)?;
                if self.pass != 3 {
                    return 0.into_lua_multi(lua);
                }
                if self.lua_device(out).is_none() {
                    return Err(mlua::Error::runtime("sj.get_byte/get_word requires DEVICE"));
                }
                let width = if method == "get_word" { 2 } else { 1 };
                if address < 0 || address > 65536 - width {
                    return Err(mlua::Error::runtime("Lua memory read is outside 64K"));
                }
                if self
                    .lua_memory
                    .as_ref()
                    .is_none_or(|(len, _)| *len != out.len())
                {
                    let empty = BTreeMap::new();
                    let seed = self.forward.as_ref().map_or(&empty, |f| &f.seed);
                    let bytes = crate::engine::lua_memory_snapshot(out, seed, self.ext.is_some())
                        .map_err(asm_error)?;
                    self.lua_memory = Some((out.len(), bytes));
                }
                let bytes = &self.lua_memory.as_ref().expect("snapshot above").1;
                let value = i64::from(bytes[address as usize])
                    | if width == 2 {
                        i64::from(bytes[address as usize + 1]) << 8
                    } else {
                        0
                    };
                value.into_lua_multi(lua)
            }
            "current_address" => self.pc.unwrap_or(0).into_lua_multi(lua),
            "get_modules" => self
                .scopes
                .prefix()
                .trim_end_matches('.')
                .into_lua_multi(lua),
            "error_count" => 0.into_lua_multi(lua),
            "warning_count" => self.lua_warnings.len().into_lua_multi(lua),
            "shellexec" => Err(mlua::Error::runtime(
                "sj.shellexec is disabled in the Lua sandbox",
            )),
            "exit" | "error" => {
                let text = args
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<mlua::Result<Vec<_>>>()?
                    .join(": ");
                Err(mlua::Error::runtime(format!("sj.{method}: {text}")))
            }
            "print" => {
                let stringify: mlua::Function = lua.globals().get("tostring")?;
                let text = args
                    .into_iter()
                    .map(|v| stringify.call::<String>(v))
                    .collect::<mlua::Result<Vec<_>>>()?
                    .join("\t");
                self.lua_warnings.push(Warning {
                    line,
                    file: self.current_file,
                    message: text,
                    kind: crate::engine::WarningKind::Note,
                });
                ().into_lua_multi(lua)
            }
            "warning" => {
                let (message, value) = <(String, Option<String>)>::from_lua_multi(args, lua)?;
                {
                    self.lua_warnings.push(Warning {
                        line,
                        file: self.current_file,
                        message: value
                            .map_or_else(|| message.clone(), |value| format!("{message}: {value}")),
                        kind: crate::engine::WarningKind::Advisory,
                    });
                }
                ().into_lua_multi(lua)
            }
            "parse_line" | "parse_code" => {
                let source = String::from_lua_multi(args, lua)?;
                let source = if method == "parse_code" {
                    format!(" {source}")
                } else {
                    source
                };
                self.lua_parse(&source, node, out).map_err(asm_error)?;
                ().into_lua_multi(lua)
            }
            "add_byte" | "add_word" => {
                let value = i64::from_lua_multi(args, lua)?;
                let source = if method == "add_byte" {
                    format!(" db {}", value & 255)
                } else {
                    format!(" dw {}", value & 65535)
                };
                self.lua_parse(&source, node, out).map_err(asm_error)?;
                ().into_lua_multi(lua)
            }
            "calc" => {
                let text = String::from_lua_multi(args, lua)?;
                let text = substitute_defines(&text, &self.defines, line).map_err(asm_error)?;
                let expr = parse_value(self.syntax, &text, line).map_err(asm_error)?;
                let op = self.scopes.qualify_op(Operation::Equ(expr), true, true);
                let Operation::Equ(expr) = op else {
                    unreachable!()
                };
                let value = self.fold_count(&expr, line).map_err(asm_error)?;
                (value as i32).into_lua_multi(lua)
            }
            "get_define" => {
                let (name, macro_args) = <(String, Option<bool>)>::from_lua_multi(args, lua)?;
                let argument = macro_args
                    .unwrap_or(false)
                    .then(|| {
                        self.lua_macro_arguments
                            .last()
                            .and_then(|args| args.get(&name))
                    })
                    .flatten();
                argument
                    .or_else(|| self.defines.get(&name))
                    .cloned()
                    .into_lua_multi(lua)
            }
            "insert_define" => {
                let (name, value) = <(String, Option<String>)>::from_lua_multi(args, lua)?;
                if !is_ident(&name) {
                    return false.into_lua_multi(lua);
                }
                self.defines
                    .insert(name, value.unwrap_or_default())
                    .is_none()
                    .into_lua_multi(lua)
            }
            "get_label" => {
                let name = String::from_lua_multi(args, lua)?;
                if !is_ident(&name) {
                    return (-1).into_lua_multi(lua);
                }
                let Operation::Equ(Expr::Sym(qualified)) =
                    self.scopes
                        .qualify_op(Operation::Equ(Expr::Sym(name.clone())), true, true)
                else {
                    unreachable!("qualifying a symbol preserves its expression kind")
                };
                let value = self
                    .consts
                    .get(&qualified)
                    .or_else(|| self.consts.get(&name))
                    .or_else(|| {
                        self.forward
                            .as_ref()
                            .and_then(|f| f.seed.get(&qualified).or_else(|| f.seed.get(&name)))
                    })
                    .copied();
                if value.is_none() && self.pass == 3 {
                    return Err(mlua::Error::runtime(format!("Label not found: {name}")));
                }
                let value = value.unwrap_or(0);
                value.into_lua_multi(lua)
            }
            "insert_label" => {
                let (name, value) = <(String, i64)>::from_lua_multi(args, lua)?;
                if !is_ident(&name) {
                    return false.into_lua_multi(lua);
                }
                let name = self.resolve_label(&name, line).map_err(asm_error)?;
                if self.consts.contains_key(&name) {
                    return false.into_lua_multi(lua);
                }
                self.consts.insert(name.clone(), value);
                out.push(Statement {
                    line,
                    file: self.current_file,
                    label: Some(name),
                    op: Some(Operation::Equ(Expr::Num(value))),
                    operand_span: None,
                    xor_mask: 0,
                    instruction_set: None,
                    extension_set: None,
                });
                true.into_lua_multi(lua)
            }
            "file_exists" => {
                let name = String::from_lua_multi(args, lua)?;
                let exists = self.multi.as_ref().is_some_and(|m| {
                    m.loader
                        .resolve_text(&name, m.map.path(self.current_file))
                        .is_some()
                        || m.loader
                            .load_binary(&name, m.map.path(self.current_file))
                            .is_ok()
                });
                exists.into_lua_multi(lua)
            }
            _ => ().into_lua_multi(lua),
        }
    }

    fn lua_device(
        &self,
        statements: &[Statement],
    ) -> Option<(crate::engine::DeviceSpec, Vec<usize>, usize)> {
        let mut state = None;
        for s in statements {
            match &s.op {
                Some(Operation::Device(spec)) => {
                    state = spec.as_ref().map(|spec| {
                        (
                            spec.clone(),
                            (0..spec.slots).collect::<Vec<_>>(),
                            spec.slots - 1,
                        )
                    })
                }
                Some(Operation::DeviceSlot(expr)) => {
                    if let Some((spec, _, current)) = &mut state
                        && let Some(slot) = eval_const(expr, &self.consts)
                            .and_then(|v| usize::try_from(v).ok())
                            .filter(|v| *v < spec.slots)
                    {
                        *current = slot;
                    }
                }
                Some(Operation::DevicePage(expr)) => {
                    if let Some((spec, slots, current)) = &mut state
                        && let Some(page) = eval_const(expr, &self.consts)
                            .and_then(|v| usize::try_from(v).ok())
                            .filter(|v| *v < spec.pages)
                    {
                        slots[*current] = page;
                    }
                }
                _ => {}
            }
        }
        state
    }
}
