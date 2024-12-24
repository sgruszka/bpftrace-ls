use btf_rs::*;

use crate::log_dbg;
use crate::log_mod::{self, BTFRE};

#[derive(Debug, Clone)]
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
    let array_name = btf.resolve_name(a).unwrap_or_default();
    type_vec.push(array_name);
}

fn get_struct_type_vec(btf: &Btf, st: &btf::Struct, type_vec: &mut Vec<String>) {
    type_vec.push("struct".to_string());
    let st_name = btf.resolve_name(st).unwrap_or_default();
    type_vec.push(st_name.clone());
}

fn resolve_struct_member(btf: &Btf, member: &btf::Member, id: u32) -> ResolvedBtfItem {
    let member_name = btf.resolve_name(member).unwrap_or_default();

    let mut item = ResolvedBtfItem {
        name: member_name,
        type_vec: Vec::new(),
        type_id: 0,
        children_vec: Vec::new(),
    };

    match btf.resolve_type_by_id(id).unwrap() {
        Type::Ptr(ptr) => resolve_pointer(btf, &ptr, &mut item),
        Type::Struct(st) => get_struct_type_vec(btf, &st, &mut item.type_vec),
        Type::Int(i) => get_int_type_vec(btf, &i, &mut item.type_vec),
        Type::Typedef(t) => get_typedef_type_vec(btf, &t, &mut item.type_vec),
        Type::Array(a) => get_array_type_vec(btf, &a, &mut item.type_vec),
        x => log_dbg!(BTFRE, "Unhandled member {:?}", x),
    };

    item
}

pub fn resolve_struct(btf: &Btf, base_id: u32) -> Option<ResolvedBtfItem> {
    let mut id = base_id;
    let mut type_vec: Vec<String> = Vec::new();

    let st = loop {
        if id == 0 {
            return None;
        }
        match btf.resolve_type_by_id(id).unwrap() {
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

fn resolve_pointer(btf: &Btf, ptr: &btf::Ptr, item: &mut ResolvedBtfItem) {
    match btf.resolve_chained_type(ptr).unwrap() {
        Type::Const(c) => {
            item.type_vec.push("const".to_string());
            // TODO: use macro to handle duplication ?
            match btf.resolve_chained_type(&c).unwrap() {
                Type::Struct(s) => {
                    item.type_id = c.get_type_id().unwrap_or_default();
                    get_struct_type_vec(btf, &s, &mut item.type_vec);
                }
                Type::Typedef(t) => {
                    item.type_id = c.get_type_id().unwrap_or_default();
                    get_typedef_type_vec(btf, &t, &mut item.type_vec);
                }
                Type::Union(_u) => {
                    item.type_id = c.get_type_id().unwrap_or_default();
                    item.type_vec.push("union".to_string()); // TODO
                }
                x => log_dbg!(BTFRE, "{} {}: Unhandled type {:?}", file!(), line!(), x),
            };
        }
        Type::Struct(s) => {
            item.type_id = ptr.get_type_id().unwrap_or_default();
            get_struct_type_vec(btf, &s, &mut item.type_vec);
        }
        Type::Void => {
            item.type_id = ptr.get_type_id().unwrap_or_default();
            item.type_vec.push("void".to_string());
        }
        Type::Typedef(t) => {
            item.type_id = ptr.get_type_id().unwrap_or_default();
            get_typedef_type_vec(btf, &t, &mut item.type_vec);
        }
        Type::Union(_u) => {
            item.type_id = ptr.get_type_id().unwrap_or_default();
            item.type_vec.push("union".to_string()); // TODO
        }
        x => log_dbg!(BTFRE, "{} {}: Unhandled type {:?}", file!(), line!(), x),
    };
    item.type_vec.push("*".to_string());
}

fn resolve_func_parameters(btf: &Btf, func: btf::Func, item: &mut ResolvedBtfItem) {
    let proto = match btf.resolve_chained_type(&func).unwrap() {
        Type::FuncProto(proto) => proto,
        x => {
            log_dbg!(BTFRE, "Resolved type is not a function proto, is {:?}", x);
            return;
        }
    };

    for i in 0..proto.parameters.len() {
        // TODO parameters diffrent than pointers to structure
        let ptr = match btf.resolve_chained_type(&proto.parameters[i]).unwrap() {
            Type::Ptr(ptr) => ptr,
            x => {
                log_dbg!(BTFRE, "Resolved type is not a pointer, is {:?}", x);
                continue;
            }
        };

        let mut parameter_item = ResolvedBtfItem {
            name: btf.resolve_name(&proto.parameters[i]).unwrap_or_default(),
            type_vec: Vec::new(),
            type_id: 0,
            children_vec: Vec::new(),
        };
        resolve_pointer(&btf, &ptr, &mut parameter_item);
        item.children_vec.push(parameter_item);
    }
}

fn resolve_parameter(btf: &Btf, param: &btf::Parameter) -> ResolvedBtfItem {
    let mut parameter_item = ResolvedBtfItem {
        name: btf.resolve_name(param).unwrap_or_default(),
        type_vec: Vec::new(),
        type_id: 0,
        children_vec: Vec::new(),
    };

    // TODO parameters diffrent than pointers to structure
    match btf.resolve_chained_type(param).unwrap() {
        Type::Ptr(ptr) => resolve_pointer(btf, &ptr, &mut parameter_item),
        // Type::Int(i) => resolve_integer(btf, &i, &mut parameter_item),
        x => {
            log_dbg!(BTFRE, "Resolved type is not a pointer, is {:?}", x);
            return parameter_item;
        }
    };
    parameter_item
}

pub fn resolve_func(btf: &Btf, name: &str) -> Option<ResolvedBtfItem> {
    log_dbg!(BTFRE, "LOOKING FOR {}", name);
    if let Err(_) = btf.resolve_types_by_name(name) {
        log_dbg!(BTFRE, "LOOKING FOR {} FAILED", name);
        return None;
    }
    let func = match btf.resolve_types_by_name(name).unwrap().pop().unwrap() {
        Type::Func(func) => func,
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
    resolve_func_parameters(btf, func, &mut item);
    Some(item)
}

pub fn setup_btf_for_module(module: &str) -> Option<Btf> {
    let btf_base = Btf::from_file("/sys/kernel/btf/vmlinux").unwrap();
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

pub fn btf_iterate_over_names_chain(
    btf: &Btf,
    func: ResolvedBtfItem,
    names_chain: &Vec<&str>,
) -> Option<ResolvedBtfItem> {
    let mut names_iter = names_chain.iter().peekable();

    if let Some(first_name) = names_iter.next() {
        let func_proto = match btf.resolve_type_by_id(func.type_id).unwrap() {
            Type::FuncProto(proto) => proto,
            x => {
                log_dbg!(BTFRE, "Resolved type is not a function proto, is {:?}", x);
                return None;
            }
        };

        let first_param = if let Some(param) = func_proto
            .parameters
            .iter()
            .find(|&p| btf.resolve_name(p).unwrap().eq(first_name))
        {
            param
        } else {
            return None;
        };

        if names_iter.peek().is_none() {
            let resolved_param = resolve_parameter(btf, &first_param);
            // TODO Union
            if let Some(mut r) = resolve_struct(btf, resolved_param.type_id) {
                // TODO pass full resolved_param to struct to keep type
                assert!(r.name == resolved_param.name);
                r.type_vec = resolved_param.type_vec;
                return Some(r);
            } else {
                return None;
            }
        }

        let mut type_id = first_param.get_type_id().unwrap_or_default();
        for name in names_iter {
            loop {
                if type_id == 0 {
                    // TODO error
                    return None;
                }
                match btf.resolve_type_by_id(type_id).unwrap() {
                    Type::Const(c) => {
                        type_id = c.get_type_id().unwrap_or_default();
                        continue;
                    }
                    Type::Ptr(ptr) => {
                        type_id = ptr.get_type_id().unwrap_or_default();
                        if *name == "->" {
                            break;
                        } else {
                            return None;
                        }
                    }
                    Type::Struct(st) => {
                        let member = if let Some(m) = st
                            .members
                            .iter()
                            .find(|&m| btf.resolve_name(m).unwrap().eq(name))
                        {
                            m
                        } else {
                            return None;
                        };
                        type_id = member.get_type_id().unwrap_or_default();
                        break;
                    }
                    // TODO
                    // Type::Int(i) =>(),  /* get_int_type_vec(btf, &i, &mut item.type_vec), */
                    // Type::Typedef(t) =>(),  /* get_typedef_type_vec(btf, &t, &mut item.type_vec), */
                    // Type::Array(a) =>(),  /* get_array_type_vec(btf, &a, &mut item.type_vec), */
                    x => {
                        log_dbg!(BTFRE, "Unhandled type {:?}", x);
                        return None;
                    }
                }
            }
        }

        return resolve_struct(btf, type_id);
    } else {
        return Some(func);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_load_module() {
        let btf1 = setup_btf_for_module("vmlinux");
        match btf1 {
            Some(_) => assert!(true),
            None => assert!(false),
        }
        let btf2 = setup_btf_for_module("blabla713h");
        match btf2 {
            Some(_) => assert!(false),
            None => assert!(true),
        }
    }

    #[test]
    fn test_resolve() {
        let btf = setup_btf_for_module("vmlinux").unwrap();

        let r = resolve_func(&btf, "alloc_pid").unwrap();
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
    fn test_iterate_over_dreference_chain() {
        // vfs_open: path->dentry->d_inode->i_uid
        let btf = setup_btf_for_module("vmlinux").unwrap();

        let base = resolve_func(&btf, "vfs_open").unwrap();

        let resolved = btf_iterate_over_names_chain(&btf, base.clone(), &Vec::new()).unwrap();
        assert!(resolved.name == "vfs_open");
        assert!(resolved.children_vec.len() == 2);
        assert!(resolved.children_vec[0].name == "path");

        let names_chain = vec!["path"];
        let resolved = btf_iterate_over_names_chain(&btf, base.clone(), &names_chain).unwrap();
        assert!(resolved.name == "path");
        assert!(resolved.type_vec == vec!["const", "struct", "path", "*"]);
        assert!(resolved.children_vec.len() > 0);

        let names_chain = vec!["path", "->", "dentry", "->", "d_inode"];
        let resolved = btf_iterate_over_names_chain(&btf, base, &names_chain).unwrap();
        assert!(resolved.name == "inode");
        assert!(resolved.children_vec.len() > 0);

        let i_state = resolved
            .children_vec
            .iter()
            .find(|&r| r.name == "i_state")
            .unwrap();
        assert!(i_state.type_vec == vec!("long unsigned int"));

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
}
