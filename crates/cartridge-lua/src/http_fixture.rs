//! Opt-in deterministic HTTP for simulator checks. No socket is opened.
use crate::api::SharedAppControl;
use mlua::prelude::*;
use serde::Deserialize;
use std::{cell::RefCell, path::Path, rc::Rc};

#[derive(Clone, Deserialize)]
struct Reply {
    url_prefix: String,
    #[serde(default = "success")]
    status: u16,
    body: serde_json::Value,
    #[serde(default = "delay")]
    delay_polls: usize,
    #[serde(default)]
    elapsed_ms: f64,
}
fn success() -> u16 {
    200
}
fn delay() -> usize {
    2
}

pub fn register(lua: &Lua, path: &Path, control: SharedAppControl) -> LuaResult<()> {
    if !cartridge_core::sim::is_sim() {
        return Err(LuaError::external("HTTP fixtures require simulator mode"));
    }
    let mut replies: Vec<Reply> =
        serde_json::from_slice(&std::fs::read(path).map_err(LuaError::external)?)
            .map_err(LuaError::external)?;
    replies.sort_by_key(|r| std::cmp::Reverse(r.url_prefix.len()));
    let queue = Rc::new(RefCell::new(Vec::<(u64, Reply)>::new()));
    let next = Rc::new(std::cell::Cell::new(0u64));
    let http = lua.create_table()?;
    let q = queue.clone();
    http.set(
        "get_async",
        lua.create_function(move |_, (url, _etag): (String, Option<String>)| {
            let id = next.get() + 1;
            next.set(id);
            let reply = replies
                .iter()
                .find(|r| url.starts_with(&r.url_prefix))
                .cloned()
                .unwrap_or(Reply {
                    url_prefix: url,
                    status: 0,
                    body: "Offline fixture: request unavailable".into(),
                    delay_polls: 2,
                    elapsed_ms: 0.0,
                });
            q.borrow_mut().push((id, reply));
            Ok(id)
        })?,
    )?;
    http.set(
        "poll",
        lua.create_function(move |lua, ()| {
            let result = lua.create_table()?;
            let mut queue = queue.borrow_mut();
            for (_, reply) in queue.iter_mut() {
                reply.delay_polls = reply.delay_polls.saturating_sub(1);
            }
            let mut i = 0;
            let mut out = 1;
            while i < queue.len() {
                if queue[i].1.delay_polls > 0 {
                    i += 1;
                    continue;
                }
                let (id, reply) = queue.remove(i);
                let item = lua.create_table()?;
                item.set("id", id)?;
                item.set("ok", (200..300).contains(&reply.status))?;
                item.set("status", reply.status)?;
                item.set("elapsed_ms", reply.elapsed_ms)?;
                item.set(
                    "body",
                    match reply.body {
                        serde_json::Value::String(s) => s,
                        other => other.to_string(),
                    },
                )?;
                result.set(out, item)?;
                out += 1;
            }
            if out > 1 {
                control.request_redraw();
            }
            Ok(result)
        })?,
    )?;
    for name in ["get", "get_cached", "post", "post_async"] {
        http.set(
            name,
            lua.create_function(|_, _: mlua::Variadic<mlua::Value>| -> LuaResult<()> {
                Err(LuaError::external(
                    "Unexpected HTTP operation in simulator fixture",
                ))
            })?,
        )?;
    }
    lua.globals().set("http", http)
}
