//! Scoped Windows registry snapshots; never execute downloaded registry scripts.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RegistryRestoreMode {
    #[default]
    Merge,
    Overwrite,
}

use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    format_version: u32,
    view: String,
    path: String,
    #[serde(deserialize_with = "deserialize_root")]
    root: Option<Node>,
}

// An omitted field must not be interpreted as permission to delete the target.
fn deserialize_root<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Node>, D::Error> {
    Option::<Node>::deserialize(deserializer)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Node {
    values: BTreeMap<String, Value>,
    children: BTreeMap<String, Node>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Value {
    vtype: u32,
    bytes: Vec<u8>,
}

fn valid_component(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['\\', '/', '\0'])
        && name.encode_utf16().count() <= 255
}

/// Normalizes a supported registry subkey path.
/// # Errors
/// Rejects unsupported hives, empty components and traversal.
pub fn normalize_path(path: &str) -> Result<String, String> {
    let (hive, subkey) = path
        .split_once('\\')
        .ok_or("请选择 HKCU 或 HKLM 下的具体注册表子键")?;
    let hive = match hive.to_ascii_uppercase().as_str() {
        "HKCU" | "HKEY_CURRENT_USER" => "HKEY_CURRENT_USER",
        "HKLM" | "HKEY_LOCAL_MACHINE" => "HKEY_LOCAL_MACHINE",
        _ => return Err("仅支持 HKCU 和 HKLM 注册表子键".into()),
    };
    if !subkey.split('\\').all(valid_component) {
        return Err("注册表路径包含无效子键或路径跳转".into());
    }
    Ok(format!("{hive}\\{subkey}"))
}

fn validate_node(node: &Node) -> Result<(), String> {
    let mut names = BTreeSet::new();
    for (name, value) in &node.values {
        if name.contains('\0')
            || name.encode_utf16().count() > 16383
            || !names.insert(name.to_lowercase())
            || value.vtype > 11
            || value.vtype == 6
        {
            return Err("注册表快照包含无效值、重复名称或 REG_LINK 链接".into());
        }
    }
    names.clear();
    for (name, child) in &node.children {
        if !valid_component(name) || !names.insert(name.to_lowercase()) {
            return Err("注册表快照包含越界子键或重复名称".into());
        }
        validate_node(child)?;
    }
    Ok(())
}

fn parse_snapshot(path: &str, bytes: &[u8]) -> Result<Snapshot, String> {
    let target = normalize_path(path)?;
    let snapshot: Snapshot =
        serde_json::from_slice(bytes).map_err(|e| format!("注册表快照格式无效：{e}"))?;
    if snapshot.format_version != 1
        || snapshot.view != "64"
        || normalize_path(&snapshot.path)? != snapshot.path
        || snapshot.path.to_lowercase() != target.to_lowercase()
    {
        return Err("注册表快照版本、视图或目标路径不匹配".into());
    }
    if let Some(root) = &snapshot.root {
        validate_node(root)?;
    }
    Ok(snapshot)
}

/// Checks a snapshot before restoring it.
/// # Errors
/// Rejects malformed data, unsupported values and mismatched targets.
pub fn validate_snapshot(path: &str, bytes: &[u8]) -> Result<(), String> {
    parse_snapshot(path, bytes).map(|_| ())
}

/// Snapshots consistently use the 64-bit registry view, including from a 32-bit process.
/// # Errors
/// Fails for unsupported platforms, invalid paths or inaccessible registry keys.
pub fn export_key(path: &str) -> Result<Vec<u8>, String> {
    platform::export(path)
}

/// Caller must capture its safety snapshot before invoking this non-transactional operation.
/// Restores a validated snapshot within its configured registry subtree.
/// # Errors
/// Fails if validation, registry access or writing fails.
pub fn restore_key(path: &str, bytes: &[u8], mode: RegistryRestoreMode) -> Result<(), String> {
    let snapshot = parse_snapshot(path, bytes)?;
    platform::restore(&snapshot, mode)
}

#[cfg(not(windows))]
mod platform {
    use super::{RegistryRestoreMode, Snapshot};
    pub fn export(_: &str) -> Result<Vec<u8>, String> {
        Err("当前系统不支持 Windows 注册表备份".into())
    }
    pub fn restore(_: &Snapshot, _: RegistryRestoreMode) -> Result<(), String> {
        Err("当前系统不支持 Windows 注册表恢复".into())
    }
}

#[cfg(windows)]
mod platform {
    use super::{
        BTreeMap, Node, RegistryRestoreMode, Snapshot, Value, normalize_path, validate_node,
    };
    use std::io;
    use winreg::{
        RegKey, RegValue,
        enums::{
            HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY, KEY_WRITE,
            REG_BINARY, REG_DWORD, REG_DWORD_BIG_ENDIAN, REG_EXPAND_SZ,
            REG_FULL_RESOURCE_DESCRIPTOR, REG_LINK, REG_MULTI_SZ, REG_NONE, REG_OPTION_OPEN_LINK,
            REG_QWORD, REG_RESOURCE_LIST, REG_RESOURCE_REQUIREMENTS_LIST, REG_SZ, RegType,
        },
    };

    #[allow(clippy::needless_pass_by_value)] // Used directly as a Result::map_err adapter.
    fn error(e: io::Error) -> String {
        if e.kind() == io::ErrorKind::PermissionDenied {
            format!("注册表访问被拒绝，请检查所选子键权限（不会自动提权）：{e}")
        } else {
            format!("注册表操作失败：{e}")
        }
    }

    fn reject_link(key: &RegKey) -> Result<(), String> {
        for value in key.enum_values() {
            if value.map_err(error)?.1.vtype == REG_LINK {
                return Err("为防止越界访问，不支持 REG_LINK 注册表链接".into());
            }
        }
        Ok(())
    }

    fn open_child(parent: &RegKey, name: &str, flags: u32) -> Result<Option<RegKey>, String> {
        match parent.open_subkey_with_options_flags(
            name,
            REG_OPTION_OPEN_LINK,
            flags | KEY_WOW64_64KEY,
        ) {
            Ok(key) => {
                reject_link(&key)?;
                Ok(Some(key))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(error(e)),
        }
    }

    // Each component is opened as a link object first, so an ancestor cannot redirect traversal.
    fn open_path(path: &str, writable: bool, create: bool) -> Result<Option<RegKey>, String> {
        let (hive, rest) = path.split_once('\\').ok_or("注册表路径无效")?;
        let mut key = RegKey::predef(if hive == "HKEY_CURRENT_USER" {
            HKEY_CURRENT_USER
        } else {
            HKEY_LOCAL_MACHINE
        });
        let components: Vec<_> = rest.split('\\').collect();
        for (index, name) in components.iter().enumerate() {
            let flags = if writable && index + 1 == components.len() {
                KEY_READ | KEY_WRITE
            } else {
                KEY_READ
            };
            key = match open_child(&key, name, flags)? {
                Some(child) => child,
                None if create => {
                    let parent = key
                        .open_subkey_with_flags("", KEY_READ | KEY_WRITE | KEY_WOW64_64KEY)
                        .map_err(error)?;
                    reject_link(&parent)?;
                    let (child, _) = parent
                        .create_subkey_with_flags(name, flags | KEY_WOW64_64KEY)
                        .map_err(error)?;
                    reject_link(&child)?;
                    child
                }
                None => return Ok(None),
            };
        }
        Ok(Some(key))
    }

    fn read_node(key: &RegKey) -> Result<Node, String> {
        reject_link(key)?;
        let mut node = Node {
            values: BTreeMap::new(),
            children: BTreeMap::new(),
        };
        for item in key.enum_values() {
            let (name, value) = item.map_err(error)?;
            node.values.insert(
                name,
                Value {
                    vtype: value.vtype as u32,
                    bytes: value.bytes,
                },
            );
        }
        for item in key.enum_keys() {
            let name = item.map_err(error)?;
            let child = open_child(key, &name, KEY_READ)?.ok_or("读取期间注册表子键发生变化")?;
            node.children.insert(name, read_node(&child)?);
        }
        Ok(node)
    }

    pub fn export(path: &str) -> Result<Vec<u8>, String> {
        let path = normalize_path(path)?;
        let root = open_path(&path, false, false)?
            .as_ref()
            .map(read_node)
            .transpose()?;
        if let Some(node) = &root {
            validate_node(node)?;
        }
        serde_json::to_vec(&Snapshot {
            format_version: 1,
            view: "64".into(),
            path,
            root,
        })
        .map_err(|e| format!("无法编码注册表快照：{e}"))
    }

    fn reg_type(value: u32) -> Result<RegType, String> {
        match value {
            0 => Ok(REG_NONE),
            1 => Ok(REG_SZ),
            2 => Ok(REG_EXPAND_SZ),
            3 => Ok(REG_BINARY),
            4 => Ok(REG_DWORD),
            5 => Ok(REG_DWORD_BIG_ENDIAN),
            7 => Ok(REG_MULTI_SZ),
            8 => Ok(REG_RESOURCE_LIST),
            9 => Ok(REG_FULL_RESOURCE_DESCRIPTOR),
            10 => Ok(REG_RESOURCE_REQUIREMENTS_LIST),
            11 => Ok(REG_QWORD),
            _ => Err("不支持的注册表值类型".into()),
        }
    }

    // Delete by inspected handles and individual names; never traverse an untrusted recursive path.
    fn remove_child(parent: &RegKey, name: &str) -> Result<(), String> {
        if let Some(child) = open_child(parent, name, KEY_READ | KEY_WRITE)? {
            let names = child
                .enum_keys()
                .collect::<Result<Vec<_>, _>>()
                .map_err(error)?;
            for name in names {
                remove_child(&child, &name)?;
            }
            drop(child);
            parent
                .delete_subkey_with_flags(name, KEY_WOW64_64KEY)
                .map_err(error)?;
        }
        Ok(())
    }

    fn write_node(key: &RegKey, node: &Node, mode: RegistryRestoreMode) -> Result<(), String> {
        reject_link(key)?;
        if mode == RegistryRestoreMode::Overwrite {
            let values = key
                .enum_values()
                .collect::<Result<Vec<_>, _>>()
                .map_err(error)?;
            for (name, _) in values {
                if !node
                    .values
                    .keys()
                    .any(|n| n.to_lowercase() == name.to_lowercase())
                {
                    key.delete_value(&name).map_err(error)?;
                }
            }
            let children = key
                .enum_keys()
                .collect::<Result<Vec<_>, _>>()
                .map_err(error)?;
            for name in children {
                if !node
                    .children
                    .keys()
                    .any(|n| n.to_lowercase() == name.to_lowercase())
                {
                    remove_child(key, &name)?;
                }
            }
        }
        for (name, value) in &node.values {
            key.set_raw_value(
                name,
                &RegValue {
                    vtype: reg_type(value.vtype)?,
                    bytes: value.bytes.clone(),
                },
            )
            .map_err(error)?;
        }
        for (name, node) in &node.children {
            let child = match open_child(key, name, KEY_READ | KEY_WRITE)? {
                Some(child) => child,
                None => {
                    key.create_subkey_with_flags(name, KEY_READ | KEY_WRITE | KEY_WOW64_64KEY)
                        .map_err(error)?
                        .0
                }
            };
            write_node(&child, node, mode)?;
        }
        Ok(())
    }

    pub fn restore(snapshot: &Snapshot, mode: RegistryRestoreMode) -> Result<(), String> {
        // Inspect the entire current subtree for links before making any changes.
        if let Some(key) = open_path(&snapshot.path, false, false)? {
            read_node(&key)?;
        }
        match &snapshot.root {
            Some(node) => {
                let key = open_path(&snapshot.path, true, true)?.ok_or("无法打开注册表目标")?;
                write_node(&key, node, mode)
            }
            None if mode == RegistryRestoreMode::Overwrite => {
                let (parent, name) = snapshot.path.rsplit_once('\\').ok_or("注册表路径无效")?;
                let key = if parent.contains('\\') {
                    open_path(parent, true, false)?
                } else {
                    Some(RegKey::predef(if parent == "HKEY_CURRENT_USER" {
                        HKEY_CURRENT_USER
                    } else {
                        HKEY_LOCAL_MACHINE
                    }))
                };
                if let Some(key) = key {
                    remove_child(&key, name)?;
                }
                Ok(())
            }
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_only_explicit_supported_subkeys() {
        assert_eq!(
            normalize_path(r"HKCU\Software\Chronicle").unwrap(),
            r"HKEY_CURRENT_USER\Software\Chronicle"
        );
        assert_eq!(
            normalize_path(r"hklm\SOFTWARE\Game").unwrap(),
            r"HKEY_LOCAL_MACHINE\SOFTWARE\Game"
        );
        for path in [
            "HKCU",
            "HKLM\\",
            r"HKCU\..\System",
            r"HKCU\\Software",
            r"\\machine\HKCU\Software",
            "HKCU\\Software\0oops",
            r"HKCR\thing",
        ] {
            assert!(normalize_path(path).is_err(), "accepted {path:?}");
        }
    }

    #[test]
    fn rejects_foreign_targets_and_escaping_child_names() {
        let target = r"HKCU\Software\ChronicleTests";
        let mut value = serde_json::json!({"format_version":1,"view":"64","path":"HKEY_CURRENT_USER\\Software\\ChronicleTests","root":{"values":{},"children":{}}});
        assert!(validate_snapshot(target, &serde_json::to_vec(&value).unwrap()).is_ok());
        value["path"] = serde_json::json!("HKEY_CURRENT_USER\\Software\\Elsewhere");
        assert!(validate_snapshot(target, &serde_json::to_vec(&value).unwrap()).is_err());
        value["path"] = serde_json::json!("HKEY_CURRENT_USER\\Software\\ChronicleTests");
        value["root"]["children"]["..\\Elsewhere"] = serde_json::json!({"values":{},"children":{}});
        assert!(validate_snapshot(target, &serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[cfg(windows)]
    #[test]
    #[allow(clippy::too_many_lines)] // One isolated registry scope covers the complete roundtrip.
    fn isolated_registry_roundtrip_merge_and_overwrite_preserve_siblings() {
        use winreg::{RegKey, RegValue, enums::*};
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
                    .delete_subkey_all(&self.0);
            }
        }
        let name = format!("Software\\ChronicleTests\\{}", uuid::Uuid::new_v4());
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (base, _) = hkcu
            .create_subkey_with_flags(&name, KEY_ALL_ACCESS | KEY_WOW64_64KEY)
            .unwrap();
        let _cleanup = Cleanup(name.clone());
        let (key, _) = base.create_subkey("Target").unwrap();
        key.set_value("", &"默认值").unwrap();
        key.set_value("number", &42u32).unwrap();
        key.set_value("large", &12_345_678_901_u64).unwrap();
        key.set_raw_value(
            "bytes",
            &RegValue {
                vtype: REG_BINARY,
                bytes: vec![0, 255, 128, 1],
            },
        )
        .unwrap();
        key.create_subkey("Empty").unwrap();
        let (sibling, _) = base.create_subkey("Sibling").unwrap();
        sibling.set_value("safe", &"untouched").unwrap();
        let path = format!("HKCU\\{name}\\Target");
        let snapshot = export_key(&path).unwrap();
        key.set_value("number", &99u32).unwrap();
        key.set_value("extra", &"keep for merge").unwrap();
        key.create_subkey("New").unwrap();
        restore_key(&path, &snapshot, RegistryRestoreMode::Merge).unwrap();
        assert_eq!(key.get_value::<u32, _>("number").unwrap(), 42);
        assert_eq!(key.get_value::<String, _>("").unwrap(), "默认值");
        assert_eq!(key.get_value::<u64, _>("large").unwrap(), 12_345_678_901);
        assert_eq!(key.get_raw_value("bytes").unwrap().bytes, [0, 255, 128, 1]);
        assert!(key.get_value::<String, _>("extra").is_ok());
        assert!(key.open_subkey("New").is_ok());
        restore_key(&path, &snapshot, RegistryRestoreMode::Overwrite).unwrap();
        assert!(key.get_value::<String, _>("extra").is_err());
        assert!(key.open_subkey("New").is_err());
        assert!(key.open_subkey("Empty").is_ok());
        assert_eq!(sibling.get_value::<String, _>("safe").unwrap(), "untouched");
        let foreign = format!("HKCU\\{name}\\Sibling");
        assert!(restore_key(&foreign, &snapshot, RegistryRestoreMode::Overwrite).is_err());
        assert_eq!(sibling.get_value::<String, _>("safe").unwrap(), "untouched");
        // Reject a malformed late child before restoring even an earlier valid value.
        let mut malformed: serde_json::Value = serde_json::from_slice(&snapshot).unwrap();
        malformed["root"]["children"]["..\\Sibling"] =
            serde_json::json!({"values":{},"children":{}});
        key.set_value("number", &77u32).unwrap();
        assert!(
            restore_key(
                &path,
                &serde_json::to_vec(&malformed).unwrap(),
                RegistryRestoreMode::Overwrite
            )
            .is_err()
        );
        assert_eq!(key.get_value::<u32, _>("number").unwrap(), 77);
        assert_eq!(sibling.get_value::<String, _>("safe").unwrap(), "untouched");

        let (linked, _) = key.create_subkey("Linked").unwrap();
        linked
            .set_raw_value(
                "SymbolicLinkValue",
                &RegValue {
                    vtype: REG_LINK,
                    bytes: vec![0, 0],
                },
            )
            .unwrap();
        assert!(export_key(&format!("{path}\\Linked\\Nested")).is_err());
        assert!(restore_key(&path, &snapshot, RegistryRestoreMode::Merge).is_err());
        assert_eq!(key.get_value::<u32, _>("number").unwrap(), 77);
        linked.delete_value("SymbolicLinkValue").unwrap();

        // Missing targets are a representable safety state, and can be recreated on reinstall.
        let absent_path = format!("HKCU\\{name}\\Absent");
        let absent = export_key(&absent_path).unwrap();
        assert!(serde_json::from_slice::<serde_json::Value>(&absent).unwrap()["root"].is_null());
        let (created, _) = base.create_subkey("Absent").unwrap();
        created.set_value("keep", &1u32).unwrap();
        restore_key(&absent_path, &absent, RegistryRestoreMode::Merge).unwrap();
        assert_eq!(created.get_value::<u32, _>("keep").unwrap(), 1);
        drop(created);
        restore_key(&absent_path, &absent, RegistryRestoreMode::Overwrite).unwrap();
        assert!(base.open_subkey("Absent").is_err());
        let mut recreated: serde_json::Value = serde_json::from_slice(&snapshot).unwrap();
        recreated["path"] = serde_json::json!(format!("HKEY_CURRENT_USER\\{name}\\Absent"));
        restore_key(
            &absent_path,
            &serde_json::to_vec(&recreated).unwrap(),
            RegistryRestoreMode::Merge,
        )
        .unwrap();
        assert_eq!(
            base.open_subkey("Absent")
                .unwrap()
                .get_value::<u32, _>("number")
                .unwrap(),
            42
        );
        assert_eq!(sibling.get_value::<String, _>("safe").unwrap(), "untouched");
    }

    #[test]
    fn rejects_link_values_unknown_types_and_case_duplicate_children() {
        let path = r"HKCU\Software\ChronicleTests";
        for root in [
            serde_json::json!({"values":{"link":{"vtype":6,"bytes":[]}},"children":{}}),
            serde_json::json!({"values":{"bad":{"vtype":12,"bytes":[]}},"children":{}}),
            serde_json::json!({"values":{},"children":{"Child":{"values":{},"children":{}},"CHILD":{"values":{},"children":{}}}}),
        ] {
            let bytes = serde_json::to_vec(&serde_json::json!({"format_version":1,"view":"64","path":"HKEY_CURRENT_USER\\Software\\ChronicleTests","root":root})).unwrap();
            assert!(validate_snapshot(path, &bytes).is_err());
        }
    }

    #[test]
    fn missing_root_is_not_an_explicit_absence_snapshot() {
        let bytes = br#"{"format_version":1,"view":"64","path":"HKEY_CURRENT_USER\\Software\\ChronicleTests"}"#;
        assert!(validate_snapshot(r"HKCU\Software\ChronicleTests", bytes).is_err());
    }
}
