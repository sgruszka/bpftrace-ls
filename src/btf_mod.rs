use btf_rs::*;

use crate::log_mod::{self, BTFRE};
use crate::{log_dbg, log_err};

#[derive(Debug, Clone, Default)]
#[allow(unused_variables)]
#[allow(dead_code)]
pub struct ResolvedBtfItem {
    pub name: String,
    pub type_vec: Vec<String>,
    pub type_id: u32,
    pub children_vec: Vec<ResolvedBtfItem>,
}

fn get_int_type_vec(btf: &Btf, i: &btf::Int, type_vec: &mut Vec<String>) {
    let integer_name = btf.resolve_name(i).unwrap_or_default();
    type_vec.push(integer_name);
}

fn get_typedef_type_vec(btf: &Btf, t: &btf::Typedef, type_vec: &mut Vec<String>) {
    let typedef_name = btf.resolve_name(t).unwrap_or_default();
    type_vec.push(typedef_name);
}

fn get_array_type_vec(btf: &Btf, a: &btf::Array, type_vec: &mut Vec<String>) {
    let mut temp_item = ResolvedBtfItem::default();
    resolve_type_id(btf, a.get_type_id().unwrap_or_default(), &mut temp_item);
    type_vec.extend(temp_item.type_vec);
    type_vec.push("[]".to_string());
}

fn get_struct_type_vec(btf: &Btf, st: &btf::Struct, type_vec: &mut Vec<String>) {
    type_vec.push("struct".to_string());
    let st_name = btf.resolve_name(st).unwrap_or_default();
    type_vec.push(st_name);
}

fn get_union_type_vec(btf: &Btf, u: &btf::Union, type_vec: &mut Vec<String>) {
    type_vec.push("union".to_string());
    let u_name = btf.resolve_name(u).unwrap_or_default();
    type_vec.push(u_name);
}
fn get_enum_type_vec(btf: &Btf, e: &btf::Enum, type_vec: &mut Vec<String>) {
    let e_name = btf.resolve_name(e).unwrap_or_default();
    type_vec.push(e_name);
}

fn get_func_proto_type_vec(btf: &Btf, fp: &btf::FuncProto, type_vec: &mut Vec<String>) {
    let mut ret_item = ResolvedBtfItem::default();
    if fp.return_type_id() > 0 {
        resolve_type_id(btf, fp.return_type_id(), &mut ret_item);
    } else {
        //TODO void has type 0, so do we need to handle this case ?
        ret_item.type_vec.push("void".to_string());
    }
    let ret_type_str = ret_item.type_vec.join(" ");

    let params_str = fp
        .parameters
        .iter()
        .map(|p| {
            let mut param_item = ResolvedBtfItem::default();
            resolve_type_id(btf, p.get_type_id().unwrap_or_default(), &mut param_item);
            param_item.type_vec.join(" ")
        })
        .collect::<Vec<String>>()
        .join(", ");

    type_vec.push(format!("{} (*)( {} )", ret_type_str, params_str));
}

fn resolve_struct_member(btf: &Btf, member: &btf::Member, id: u32) -> ResolvedBtfItem {
    let member_name = btf.resolve_name(member).unwrap_or_default();

    let mut item = ResolvedBtfItem {
        name: member_name,
        type_vec: Vec::new(),
        type_id: 0,
        children_vec: Vec::new(),
    };

    match btf.resolve_type_by_id(id) {
        Ok(Type::Ptr(ptr)) => resolve_pointer(btf, &ptr, &mut item),
        Ok(Type::Struct(st)) => get_struct_type_vec(btf, &st, &mut item.type_vec),
        Ok(Type::Union(u)) => get_union_type_vec(btf, &u, &mut item.type_vec),
        Ok(Type::Int(i)) => get_int_type_vec(btf, &i, &mut item.type_vec),
        Ok(Type::Typedef(t)) => get_typedef_type_vec(btf, &t, &mut item.type_vec),
        Ok(Type::Array(a)) => get_array_type_vec(btf, &a, &mut item.type_vec),
        Ok(x) => log_dbg!(BTFRE, "Unhandled member {:?}", x),
        Err(_) => log_err!("Failed to resolve BTF id {}", id),
    };

    item
}

fn resolve_struct(btf: &Btf, base_id: u32) -> Option<ResolvedBtfItem> {
    let mut id = base_id;
    let mut type_vec: Vec<String> = Vec::new();

    let st = loop {
        if id == 0 {
            return None;
        }
        match btf.resolve_type_by_id(id).ok()? {
            Type::Const(c) => {
                type_vec.push("const".to_string());
                id = c.get_type_id().unwrap_or_default();
                continue;
            }
            Type::Ptr(ptr) => {
                type_vec.push("*".to_string());
                id = ptr.get_type_id().unwrap_or_default();
                continue;
            }
            Type::TypeTag(tt) => {
                id = tt.get_type_id().unwrap_or_default();
                continue;
            }
            Type::Struct(st) => {
                type_vec.push("struct".to_string());
                break st;
            }
            x => {
                log_dbg!(BTFRE, "Unhandled type {:?}", x);
                return None;
            }
        }
    };

    let mut children: Vec<ResolvedBtfItem> = Vec::new();

    for member in st.members.iter() {
        let id = member.get_type_id().unwrap_or_default();
        if id != 0 {
            let child = resolve_struct_member(btf, member, id);
            children.push(child);
        }
    }

    Some(ResolvedBtfItem {
        name: btf.resolve_name(&st).unwrap_or_default(),
        type_vec,
        type_id: id,
        children_vec: children,
    })
}

fn resolve_union(btf: &Btf, base_id: u32) -> Option<ResolvedBtfItem> {
    let mut id = base_id;
    let mut type_vec: Vec<String> = Vec::new();

    let u = loop {
        if id == 0 {
            return None;
        }
        match btf.resolve_type_by_id(id).ok()? {
            Type::Const(c) => {
                type_vec.push("const".to_string());
                id = c.get_type_id().unwrap_or_default();
                continue;
            }
            Type::Ptr(ptr) => {
                type_vec.push("*".to_string());
                id = ptr.get_type_id().unwrap_or_default();
                continue;
            }
            Type::TypeTag(tt) => {
                id = tt.get_type_id().unwrap_or_default();
                continue;
            }
            Type::Union(u) => {
                type_vec.push("union".to_string());
                break u;
            }
            x => {
                log_dbg!(BTFRE, "Unhandled type {:?}", x);
                return None;
            }
        }
    };

    let mut children: Vec<ResolvedBtfItem> = Vec::new();

    for member in u.members.iter() {
        let id = member.get_type_id().unwrap_or_default();
        if id != 0 {
            let child = resolve_struct_member(btf, member, id);
            children.push(child);
        }
    }

    Some(ResolvedBtfItem {
        name: btf.resolve_name(&u).unwrap_or_default(),
        type_vec,
        type_id: id,
        children_vec: children,
    })
}

fn resolve_pointer(btf: &Btf, ptr: &btf::Ptr, item: &mut ResolvedBtfItem) {
    let mut chained_type = btf.resolve_chained_type(ptr).unwrap();
    let mut tag = None;
    let mut num_ptrs = 1;

    while let Type::Ptr(p) = chained_type {
        chained_type = btf.resolve_chained_type(&p).unwrap();
        num_ptrs += 1;
    }

    let final_type = match chained_type {
        Type::Const(c) => {
            item.type_vec.push("const".to_string());
            item.type_id = c.get_type_id().unwrap_or_default();
            btf.resolve_chained_type(&c).unwrap()
        }
        Type::TypeTag(tt) => {
            tag = Some(btf.resolve_name(&tt).unwrap_or_default());
            item.type_id = tt.get_type_id().unwrap_or_default();
            btf.resolve_chained_type(&tt).unwrap()
        }
        other => {
            item.type_id = ptr.get_type_id().unwrap_or_default();
            other
        }
    };

    let is_func_ptr = matches!(final_type, Type::FuncProto(_));

    match final_type {
        Type::Struct(s) => get_struct_type_vec(btf, &s, &mut item.type_vec),
        Type::Typedef(t) => get_typedef_type_vec(btf, &t, &mut item.type_vec),
        Type::Union(u) => get_union_type_vec(btf, &u, &mut item.type_vec),
        Type::Int(i) => get_int_type_vec(btf, &i, &mut item.type_vec),
        Type::Array(a) => get_array_type_vec(btf, &a, &mut item.type_vec),
        Type::FuncProto(fp) => get_func_proto_type_vec(btf, &fp, &mut item.type_vec),
        Type::Void => {
            item.type_vec.push("void".to_string());
        }
        x => log_dbg!(BTFRE, "{} {}: Unhandled type {:?}", file!(), line!(), x),
    };

    if let Some(tag_str) = tag {
        item.type_vec.push(tag_str.to_owned());
    }

    // TODO handle num_ptrs > 1 for function pointer
    if !is_func_ptr {
        item.type_vec.push("*".repeat(num_ptrs).to_string());
    }
}

fn resolve_type_id(btf: &Btf, id: u32, param_item: &mut ResolvedBtfItem) {
    let mut type_id = id;
    loop {
        if type_id == 0 {
            break;
        }

        match btf.resolve_type_by_id(type_id) {
            Ok(Type::Const(c)) => {
                param_item.type_vec.push("const".to_string());
                type_id = c.get_type_id().unwrap_or_default();
                continue;
            }
            Ok(Type::Volatile(v)) => {
                param_item.type_vec.push("volatile".to_string());
                type_id = v.get_type_id().unwrap_or_default();
                continue;
            }
            Ok(Type::Ptr(ptr)) => {
                param_item.type_id = ptr.get_type_id().unwrap_or_default();
                resolve_pointer(btf, &ptr, param_item);
                break;
            }
            Ok(Type::Typedef(td)) => {
                param_item.type_id = td.get_type_id().unwrap_or_default();
                get_typedef_type_vec(btf, &td, &mut param_item.type_vec);
                break;
            }
            Ok(Type::Int(i)) => {
                param_item.type_id = i.get_type_id().unwrap_or_default();
                get_int_type_vec(btf, &i, &mut param_item.type_vec);
                break;
            }
            Ok(Type::Enum(e)) => {
                param_item.type_id = e.get_type_id().unwrap_or_default();
                get_enum_type_vec(btf, &e, &mut param_item.type_vec);
                break;
            }
            Ok(x) => {
                log_dbg!(BTFRE, "Unhandled type {:?}", x);
                break;
            }
            Err(_) => log_err!("Failed to resolve BTF id {}", id),
        }
    }
}

fn resolve_func_parameters(
    btf: &Btf,
    func: btf::Func,
    item: &mut ResolvedBtfItem,
    need_retval: bool,
) {
    let proto = match btf.resolve_chained_type(&func).ok() {
        Some(Type::FuncProto(proto)) => proto,
        x => {
            log_dbg!(BTFRE, "Resolved type is not a function proto, is {:?}", x);
            return;
        }
    };

    let ret_type_id = proto.return_type_id();

    for param in proto.parameters {
        let mut param_item = ResolvedBtfItem {
            name: btf.resolve_name(&param).unwrap_or_default(),
            type_vec: Vec::new(),
            type_id: 0,
            children_vec: Vec::new(),
        };

        let id = param.get_type_id().unwrap_or_default();
        resolve_type_id(btf, id, &mut param_item);
        item.children_vec.push(param_item);
    }

    if ret_type_id > 0 && need_retval {
        let mut ret_item = ResolvedBtfItem {
            name: "retval".to_string(),
            type_vec: Vec::new(),
            type_id: 0,
            children_vec: Vec::new(),
        };
        resolve_type_id(btf, ret_type_id, &mut ret_item);
        item.children_vec.push(ret_item);
    }
}

fn resolve_parameter(btf: &Btf, param: &btf::Parameter) -> ResolvedBtfItem {
    let mut parameter_item = ResolvedBtfItem {
        name: btf.resolve_name(param).unwrap_or_default(),
        type_vec: Vec::new(),
        type_id: 0,
        children_vec: Vec::new(),
    };

    // TODO other parameters types, merge with resolve_struct_member
    match btf.resolve_chained_type(param) {
        Ok(Type::Ptr(ptr)) => resolve_pointer(btf, &ptr, &mut parameter_item),
        Ok(Type::Int(i)) => get_int_type_vec(btf, &i, &mut parameter_item.type_vec),
        Ok(Type::Typedef(t)) => get_typedef_type_vec(btf, &t, &mut parameter_item.type_vec),
        Ok(Type::Union(u)) => get_union_type_vec(btf, &u, &mut parameter_item.type_vec),
        Ok(x) => {
            log_dbg!(BTFRE, "Unhandled type {:?}", x);
            return parameter_item;
        }
        Err(_) => log_err!("Failed to resolve BTF parameter {:?}", param),
    };
    parameter_item
}

fn is_pointer_type(btf: &Btf, base_id: u32) -> bool {
    let mut id = base_id;
    loop {
        if id == 0 {
            return false;
        }

        match btf.resolve_type_by_id(id).unwrap_or(Type::Void) {
            Type::Const(c) => {
                id = c.get_type_id().unwrap_or_default();
                continue;
            }
            Type::Volatile(v) => {
                id = v.get_type_id().unwrap_or_default();
                continue;
            }
            Type::Ptr(_) => {
                return true;
            }
            _ => {
                return false;
            }
        }
    }
}

pub fn btf_resolve_func(btf: &Btf, name: &str, need_retval: bool) -> Option<ResolvedBtfItem> {
    log_dbg!(BTFRE, "Looking for {}", name);
    if let Err(err) = btf.resolve_types_by_name(name) {
        log_err!("Looking for {} failed with {:?}", name, err);
        return None;
    }

    let mut func_vec = btf.resolve_types_by_name(name).ok()?;
    // TODO what todo if more then one function ?
    if func_vec.len() != 1 {
        return None;
    }

    let func = match func_vec.pop() {
        Some(Type::Func(func)) => func,
        x => {
            log_dbg!(BTFRE, "Resolved type is not a function, it's {:?}", x);
            return None;
        }
    };
    let mut item = ResolvedBtfItem {
        name: "".to_string(),
        type_vec: Vec::new(),
        type_id: 0,
        children_vec: Vec::new(),
    };
    item.type_id = func.get_type_id().unwrap_or_default();
    item.name = btf.resolve_name(&func).unwrap_or_default();
    item.type_vec = vec!["func".to_string()]; // TODO function prototype evaluation
    resolve_func_parameters(btf, func, &mut item, need_retval);
    Some(item)
}

pub fn btf_setup_module(module: &str) -> Option<Btf> {
    let btf_base = Btf::from_file("/sys/kernel/btf/vmlinux").ok()?;
    if module.is_empty() || module == "vmlinux" {
        return Some(btf_base);
    }

    let path = "/sys/kernel/btf/".to_string() + module;
    if let Ok(btf) = Btf::from_split_file(&path, &btf_base) {
        log_dbg!(BTFRE, "Loaded btf for {}", path);
        return Some(btf);
    }

    None
}

fn chain_str_to_tokens(names_chain: &str) -> Vec<&str> {
    let mut res: Vec<&str> = Vec::new();

    let mut start_idx = 0;
    let mut end_idx = 0;

    for (i, c) in names_chain.char_indices() {
        match c {
            '.' => {
                res.push(&names_chain[start_idx..i]);
                res.push(".");
                start_idx = i + 1;
            }
            '-' => {
                res.push(&names_chain[start_idx..i]);
                start_idx = i + 1;
            }
            '>' => {
                res.push("->");
                start_idx = i + 1;
            }
            _ => end_idx = i,
        };
    }

    if end_idx != 0 && start_idx <= end_idx {
        res.push(&names_chain[start_idx..=end_idx]);
    }

    res
}

pub fn btf_iterate_over_names_chain(
    btf: &Btf,
    func: &ResolvedBtfItem,
    names_chain_str: &str,
) -> Option<ResolvedBtfItem> {
    let mut names_chain_vec = chain_str_to_tokens(names_chain_str);
    if names_chain_vec.len() >= 2 && names_chain_vec[0] == "args" && names_chain_vec[1] == "." {
        // Remove "args."
        names_chain_vec.remove(0);
        names_chain_vec.remove(0);
    }

    let mut names_iter = names_chain_vec.iter().peekable();

    if let Some(first_name) = names_iter.next() {
        let func_proto = match btf.resolve_type_by_id(func.type_id).ok() {
            Some(Type::FuncProto(proto)) => proto,
            x => {
                log_dbg!(BTFRE, "Resolved type is not a function proto, is {:?}", x);
                return None;
            }
        };

        let first_param = func_proto
            .parameters
            .iter()
            .find(|&p| btf.resolve_name(p).unwrap().eq(first_name))?;

        if names_iter.peek().is_none() {
            let resolved_param = resolve_parameter(btf, first_param);
            if let Some(mut r) = resolve_struct(btf, resolved_param.type_id) {
                r.type_vec = resolved_param.type_vec;
                return Some(r);
            } else if let Some(mut r) = resolve_union(btf, resolved_param.type_id) {
                r.type_vec = resolved_param.type_vec;
                return Some(r);
            } else {
                return Some(resolved_param);
            }
        }

        // Handle struct/union members: use -> for pointrs and . for direct access
        let mut type_id = first_param.get_type_id().unwrap_or_default();
        let mut last_name = *first_name;
        let mut names_iter_peek = names_iter.peekable();
        while let Some(op) = names_iter_peek.next() {
            let is_pointer = is_pointer_type(btf, type_id);
            if *op == "->" {
                if !is_pointer {
                    return None;
                }
            } else if *op == "." {
                if is_pointer {
                    return None;
                }
            } else {
                return None;
            }

            let member_name = if let Some(name) = names_iter_peek.next() {
                name
            } else {
                if names_chain_vec.last() == Some(&"->") || names_chain_vec.last() == Some(&".") {
                    break;
                }
                return None;
            };

            loop {
                if type_id == 0 {
                    // TODO error
                    return None;
                }

                match btf.resolve_type_by_id(type_id) {
                    Ok(Type::Const(c)) => {
                        type_id = c.get_type_id().unwrap_or_default();
                        continue;
                    }
                    Ok(Type::Ptr(ptr)) => {
                        type_id = ptr.get_type_id().unwrap_or_default();
                        continue;
                    }
                    Ok(Type::Struct(st)) => {
                        let member = st
                            .members
                            .iter()
                            .find(|&m| btf.resolve_name(m).unwrap().eq(member_name))?;

                        type_id = member.get_type_id().unwrap_or_default();
                        last_name = member_name;
                        break;
                    }
                    Ok(Type::Union(u)) => {
                        let member = u
                            .members
                            .iter()
                            .find(|&m| btf.resolve_name(m).unwrap().eq(member_name))?;

                        type_id = member.get_type_id().unwrap_or_default();
                        last_name = member_name;
                        break;
                    }
                    // TODO
                    // Type::Int(i) =>(),  /* get_int_type_vec(btf, &i, &mut item.type_vec), */
                    // Type::Typedef(t) =>(),  /* get_typedef_type_vec(btf, &t, &mut item.type_vec), */
                    // Type::Array(a) =>(),  /* get_array_type_vec(btf, &a, &mut item.type_vec), */
                    Ok(x) => {
                        log_dbg!(BTFRE, "Unhandled type {:?}", x);
                        return None;
                    }
                    Err(_) => log_err!("Failed to resolve BTF type_id {}", type_id),
                }
            }
        }

        if let Some(mut r) = resolve_struct(btf, type_id) {
            r.name = last_name.to_string();
            return Some(r);
        }
        if let Some(mut r) = resolve_union(btf, type_id) {
            r.name = last_name.to_string();
            return Some(r);
        }
        let mut item = ResolvedBtfItem {
            name: last_name.to_string(),
            ..Default::default()
        };
        resolve_type_id(btf, type_id, &mut item);
        Some(item)
    } else {
        Some(func.clone())
    }
}

#[allow(clippy::len_zero)]
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_load_module() {
        let btf1 = btf_setup_module("vmlinux");
        assert!(btf1.is_some());

        let btf2 = btf_setup_module("Xblabla713h");
        assert!(btf2.is_none());
    }

    #[test]
    fn test_chain_str_to_tokens() {
        assert!(chain_str_to_tokens("args") == vec!["args"]);
        assert!(chain_str_to_tokens("args.") == vec!["args", "."]);
        assert!(chain_str_to_tokens("xxx->yyy") == vec!["xxx", "->", "yyy"]);
        assert!(chain_str_to_tokens("a.b.c.d") == vec!["a", ".", "b", ".", "c", ".", "d"]);
        assert!(
            chain_str_to_tokens("args.f1.f2->f3") == vec!["args", ".", "f1", ".", "f2", "->", "f3"]
        );
    }
    #[test]
    fn test_resolve() {
        let btf = btf_setup_module("vmlinux").unwrap();

        let r = btf_resolve_func(&btf, "alloc_pid", true).unwrap();
        assert!(r.name == "alloc_pid");
        assert!(r.children_vec[0].name == "ns");

        let pid_namespace = resolve_struct(&btf, r.children_vec[0].type_id).unwrap();
        assert!(pid_namespace.name == "pid_namespace");

        let pid_cachep = pid_namespace
            .children_vec
            .iter()
            .find(|v| v.name == "pid_cachep")
            .unwrap();
        assert!(pid_cachep.name == "pid_cachep");
        assert!(pid_cachep.type_vec[1] == "kmem_cache");
    }

    #[test]
    fn test_iterate_over_mixed_chain() {
        // alloc_pid: ns->rcu.next->func
        let btf = btf_setup_module("vmlinux").unwrap();

        let base = btf_resolve_func(&btf, "alloc_pid", true).unwrap();

        let resolved = btf_iterate_over_names_chain(&btf, &base, "args.ns->rcu.next").unwrap();
        assert!(resolved.name == "next");
        assert!(resolved.children_vec[0].name == "next");

        let resolved_func = btf_iterate_over_names_chain(&btf, &base, "args.ns->rcu.func").unwrap();
        assert!(resolved_func.name == "func");
        assert_eq!(
            resolved_func.type_vec,
            vec!["void (*)( struct callback_head * )"]
        );

        let resolved_fail = btf_iterate_over_names_chain(&btf, &base, "args.ns->rcu->next");
        assert!(resolved_fail.is_none());
    }

    #[test]
    fn test_iterate_over_dreference_chain() {
        // vfs_open: path->dentry->d_inode->i_uid
        let btf = btf_setup_module("vmlinux").unwrap();

        let base = btf_resolve_func(&btf, "vfs_open", true).unwrap();
        assert!(base.name == "vfs_open");

        let resolved = btf_iterate_over_names_chain(&btf, &base, "").unwrap();
        assert!(resolved.name == "vfs_open");
        assert!(resolved.children_vec.len() == 3);
        assert!(resolved.children_vec[0].name == "path");
        assert!(resolved.children_vec[2].name == "retval");

        let resolved = btf_iterate_over_names_chain(&btf, &base, "args.path").unwrap();
        assert!(resolved.name == "path");
        assert!(resolved.type_vec == vec!["const", "struct", "path", "*"]);
        assert!(resolved.children_vec.len() > 0);

        let resolved =
            btf_iterate_over_names_chain(&btf, &base, "args.path->dentry->d_inode").unwrap();
        assert!(resolved.name == "d_inode");
        assert!(resolved.children_vec.len() > 0);

        let resolved_fail = btf_iterate_over_names_chain(&btf, &base, "args.path.dentry");
        assert!(resolved_fail.is_none());

        let i_state = resolved
            .children_vec
            .iter()
            .find(|&r| r.name == "i_ino")
            .unwrap();
        assert!(i_state.type_vec == vec!("unsigned long"));

        let i_count = resolved
            .children_vec
            .iter()
            .find(|&r| r.name == "i_count")
            .unwrap();
        assert!(i_count.type_vec == vec!("atomic_t"));

        let i_uid = resolved
            .children_vec
            .iter()
            .find(|&r| r.name == "i_uid")
            .unwrap();
        assert!(i_uid.type_vec == vec!("kuid_t"));
    }

    #[test]
    // #[ignore]
    fn test_resolve_rt2800_link_tuner() {
        let btf = match btf_setup_module("rt2800lib") {
            Some(btf) => btf,
            None => {
                eprintln!("\x1b[33mskipped\x1b[0m: rt2800lib module not loaded");
                return;
            }
        };
        let base = btf_resolve_func(&btf, "rt2800_link_tuner", true).unwrap();
        let resolved = btf_iterate_over_names_chain(&btf, &base, "qual->").unwrap();

        let vgc_level = resolved
            .children_vec
            .iter()
            .find(|&r| r.name == "vgc_level")
            .unwrap();
        assert!(vgc_level.type_vec == vec!("u8"));
    }

    #[test]
    fn test_resolve_k_itimer_union() {
        let btf = btf_setup_module("vmlinux").unwrap();
        let base = btf_resolve_func(&btf, "posixtimer_send_sigqueue", true).unwrap();
        let resolved = btf_iterate_over_names_chain(&btf, &base, "args.tmr->it").unwrap();

        assert!(resolved.type_vec.iter().any(|s| s == "union"));

        let cpu_member = resolved
            .children_vec
            .iter()
            .find(|&r| r.name == "cpu")
            .unwrap();

        assert!(cpu_member.type_vec.iter().any(|s| s == "cpu_timer"));
        assert!(cpu_member.type_vec.iter().any(|s| s == "struct"));

        let real_member = resolved
            .children_vec
            .iter()
            .find(|&r| r.name == "real")
            .unwrap();

        assert!(real_member.type_vec.iter().any(|s| s == "struct"));
    }

    #[test]
    fn test_resolve_ieee80211_hw_array_in_struct() {
        // This test requires mac80211 module to be loaded
        let btf = match btf_setup_module("mac80211") {
            Some(btf) => btf,
            None => {
                eprintln!("\x1b[33mskipped\x1b[0m: mac80211 module not loaded");
                return;
            }
        };
        let base = btf_resolve_func(&btf, "ieee80211_register_hw", true).unwrap();

        // The argument is struct ieee80211_hw *hw
        let hw = btf_iterate_over_names_chain(&btf, &base, "args.hw").unwrap();

        let flags = hw.children_vec.iter().find(|&r| r.name == "flags").unwrap();

        assert_eq!(flags.type_vec, vec!["unsigned long", "[]"]);
    }
}
