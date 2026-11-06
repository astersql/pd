// Copyright 2026 AsterSQL.
//! Local machine observations. The admitted Darwin contract is the pinned
//! sysutil/gopsutil non-CGO collector; unavailable resources are omitted.
use astersql_pd_proto::diagnosticspb::{ServerInfoItem, ServerInfoPair, ServerInfoResponse};
use tonic::Status;

pub(crate) fn collect(tp: i32) -> Result<ServerInfoResponse, Status> {
    if !(0..=3).contains(&tp) {
        return Ok(ServerInfoResponse { items: Vec::new() });
    }
    #[cfg(target_os = "macos")]
    {
        let mut items = Vec::new();
        if tp == 0 || tp == 3 {
            items.extend(darwin::load());
        }
        if tp == 0 || tp == 1 {
            items.extend(darwin::hardware());
        }
        if tp == 0 || tp == 2 {
            items.extend(darwin::system());
        }
        items.sort_by(|a, b| (&a.tp, &a.name).cmp(&(&b.tp, &b.name)));
        Ok(ServerInfoResponse { items })
    }
    #[cfg(not(target_os = "macos"))]
    Err(Status::unimplemented(
        "ServerInfo platform has not been admitted",
    ))
}
fn item(tp: &str, name: &str, pairs: Vec<(&str, String)>) -> ServerInfoItem {
    ServerInfoItem {
        tp: tp.into(),
        name: name.into(),
        pairs: pairs
            .into_iter()
            .map(|(key, value)| ServerInfoPair {
                key: key.into(),
                value,
            })
            .collect(),
    }
}
#[cfg(target_os = "macos")]
fn output(command: &str, args: &[&str]) -> Option<String> {
    use std::{
        io::Read,
        os::{fd::OwnedFd, unix::net::UnixStream},
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };
    // Both Go exec.CombinedOutput and gopsutil's Invoke share one stdout/stderr
    // writer. A shared socket preserves that byte order without a temp file or
    // reader thread. Drain it while the child runs, including outputs >64KiB.
    let (mut reader, writer) = UnixStream::pair().ok()?;
    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdout(Stdio::from(OwnedFd::from(writer.try_clone().ok()?)))
        .stderr(Stdio::from(OwnedFd::from(writer)));
    struct Reap(Child);
    impl Drop for Reap {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }
    let mut child = Reap(cmd.spawn().ok()?);
    drop(cmd); // Close the parent's copies of the child-side socket.
    reader.set_nonblocking(true).ok()?;
    let timeout =
        matches!(command, "vm_stat" | "netstat" | "ifconfig").then_some(Duration::from_secs(3));
    let start = Instant::now();
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    let mut eof = false;
    loop {
        let mut blocked = false;
        match reader.read(&mut buffer) {
            Ok(0) => eof = true,
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => blocked = true,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
        if let Some(status) = child.0.try_wait().ok()? {
            if eof {
                return status
                    .success()
                    .then(|| String::from_utf8_lossy(&bytes).into_owned());
            }
        }
        // sysutil's sysctl fallback has no command timeout; gopsutil's Invoke
        // has a three-second background context independent of the RPC context.
        if timeout.is_some_and(|limit| start.elapsed() >= limit) {
            return None;
        }
        if blocked || eof {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
// Go's fallback splits at every colon and keeps just the first two fields.
fn system_pairs(text: &str) -> Vec<ServerInfoPair> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split(':');
            Some(ServerInfoPair {
                key: fields.next()?.into(),
                value: fields.next()?.trim().into(),
            })
        })
        .collect()
}
// All four recognized counters are parsed by gopsutil, even though only free
// and inactive contribute to Available. Absent counters keep their zero value.
fn available_memory(text: &str, page: u64) -> Option<u64> {
    let mut free = 0u64;
    let mut inactive = 0u64;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if [
            "Pages free",
            "Pages inactive",
            "Pages active",
            "Pages wired down",
        ]
        .contains(&key)
        {
            let count = value.trim_matches([' ', '.']).parse::<u64>().ok()?;
            if key == "Pages free" {
                free = count.wrapping_mul(page);
            }
            if key == "Pages inactive" {
                inactive = count.wrapping_mul(page);
            }
        }
    }
    Some(free.wrapping_add(inactive))
}
#[cfg(target_os = "macos")]
mod darwin {
    use super::*;
    use std::{
        collections::BTreeMap,
        ffi::{CStr, CString},
        mem::{MaybeUninit, size_of},
        net::{Ipv4Addr, Ipv6Addr},
        ptr,
    };

    // Each sysctl fills a correctly aligned, initialized POD value. Callers use
    // only scalar/array types with layouts matching the Darwin ABI.
    fn sysctl<T: Copy + Default>(name: &str) -> Option<T> {
        let name = CString::new(name).ok()?;
        let mut value = T::default();
        let mut len = size_of::<T>();
        // SAFETY: name is NUL terminated, output points to len writable bytes;
        // no input buffer is passed and the returned size is checked.
        let rc = unsafe {
            libc::sysctlbyname(
                name.as_ptr(),
                (&mut value as *mut T).cast(),
                &mut len,
                ptr::null_mut(),
                0,
            )
        };
        (rc == 0 && len == size_of::<T>()).then_some(value)
    }
    fn ctext(bytes: &[libc::c_char]) -> String {
        let bytes: Vec<u8> = bytes
            .iter()
            .take_while(|b| **b != 0)
            .map(|b| *b as u8)
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }
    fn virtual_memory() -> Option<(u64, u64)> {
        let total = sysctl::<u64>("hw.memsize")?;
        let text = output("vm_stat", &[])?;
        // gopsutil multiplies vm_stat counters by unix.Getpagesize, not the
        // header's kernel page size. Under Rosetta these are 4096 vs 16384.
        // SAFETY: sysconf takes a scalar selector and returns no borrowed data.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if page <= 0 {
            return None;
        }
        let page = page as u64;
        Some((total, available_memory(&text, page)?))
    }

    pub(super) fn hardware() -> Vec<ServerInfoItem> {
        let mut items = Vec::new();
        let arch = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            "x86_64" => "amd64",
            other => other,
        };
        items.push(item(
            "cpu",
            "cpu",
            vec![
                ("cpu-arch", arch.into()),
                (
                    "cpu-logical-cores",
                    sysctl::<u32>("hw.logicalcpu").unwrap_or(1).to_string(),
                ),
                (
                    "cpu-physical-cores",
                    sysctl::<u32>("hw.physicalcpu")
                        .unwrap_or_default()
                        .to_string(),
                ),
                (
                    "cpu-frequency",
                    format!(
                        "{:.2}MHz",
                        sysctl::<u64>("hw.cpufrequency").unwrap_or_default() as f64 / 1_000_000.0
                    ),
                ),
                (
                    "cache",
                    sysctl::<u32>("machdep.cpu.cache.size")
                        .unwrap_or_default()
                        .to_string(),
                ),
            ],
        ));
        if let Some((total, _)) = virtual_memory() {
            items.push(item(
                "memory",
                "memory",
                vec![("capacity", total.to_string())],
            ));
        }
        items.extend(disks());
        items.extend(interfaces());
        items
    }
    fn disks() -> Vec<ServerInfoItem> {
        // SAFETY: null/zero obtains capacity; the initialized array below owns
        // the buffer passed to getfsstat. Clamp to its allocated size on races.
        let count = unsafe { libc::getfsstat(ptr::null_mut(), 0, libc::MNT_WAIT) };
        if count <= 0 {
            return Vec::new();
        }
        let mut stats: Vec<libc::statfs> = (0..count)
            .map(|_| unsafe { MaybeUninit::zeroed().assume_init() })
            .collect();
        let count = unsafe {
            libc::getfsstat(
                stats.as_mut_ptr(),
                (stats.len() * size_of::<libc::statfs>()) as i32,
                libc::MNT_WAIT,
            )
        };
        if count < 0 {
            return Vec::new();
        }
        let mut result = Vec::new();
        for stat in stats.iter().take(count as usize) {
            let device = ctext(&stat.f_mntfromname);
            let Some(name) = device.strip_prefix("/dev/") else {
                continue;
            };
            let path = ctext(&stat.f_mntonname);
            let Ok(cpath) = CString::new(path.as_str()) else {
                continue;
            };
            let mut usage: libc::statfs = unsafe { MaybeUninit::zeroed().assume_init() };
            // SAFETY: cpath is terminated and usage is a writable statfs.
            if unsafe { libc::statfs(cpath.as_ptr(), &mut usage) } != 0 {
                continue;
            }
            let mut opts = vec![if stat.f_flags & libc::MNT_RDONLY as u32 != 0 {
                "ro"
            } else {
                "rw"
            }];
            for (flag, text) in [
                (libc::MNT_SYNCHRONOUS, "sync"),
                (libc::MNT_NOEXEC, "noexec"),
                (libc::MNT_NOSUID, "nosuid"),
                (libc::MNT_UNION, "union"),
                (libc::MNT_ASYNC, "async"),
                (libc::MNT_DONTBROWSE, "nobrowse"),
                (libc::MNT_AUTOMOUNTED, "automounted"),
                (libc::MNT_JOURNALED, "journaled"),
                (libc::MNT_MULTILABEL, "multilabel"),
                (libc::MNT_NOATIME, "noatime"),
                (libc::MNT_NODEV, "nodev"),
            ] {
                if stat.f_flags & flag as u32 != 0 {
                    opts.push(text);
                }
            }
            let total = usage.f_blocks * usage.f_bsize as u64;
            let free = usage.f_bavail * usage.f_bsize as u64;
            let used = (usage.f_blocks - usage.f_bfree) * usage.f_bsize as u64;
            let ratio = if used + free == 0 {
                0.0
            } else {
                used as f64 / (used + free) as f64
            };
            result.push(item(
                "disk",
                name,
                vec![
                    ("fstype", ctext(&stat.f_fstypename)),
                    ("opts", opts.join(",")),
                    ("path", path),
                    ("total", total.to_string()),
                    ("free", free.to_string()),
                    ("used", used.to_string()),
                    (
                        "free-percent",
                        format!("{:.2}", (100.0 - ratio * 100.0) / 100.0),
                    ),
                    ("used-percent", format!("{:.2}", (ratio * 100.0) / 100.0)),
                ],
            ));
        }
        result
    }
    fn interfaces() -> Vec<ServerInfoItem> {
        let mut head = ptr::null_mut();
        // SAFETY: getifaddrs writes an owned list, freed by Guard on every exit.
        if unsafe { libc::getifaddrs(&mut head) } != 0 {
            return Vec::new();
        }
        struct Guard(*mut libc::ifaddrs);
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe { libc::freeifaddrs(self.0) }
            }
        }
        let _guard = Guard(head);
        let mut map: BTreeMap<String, (u32, String, Vec<String>)> = BTreeMap::new();
        let mut p = head;
        while !p.is_null() {
            // SAFETY: all list nodes, names and sockaddr storage remain valid
            // until the guard is dropped. Cast only after checking sa_family.
            let a = unsafe { &*p };
            let name = unsafe { CStr::from_ptr(a.ifa_name) }
                .to_string_lossy()
                .into_owned();
            let e = map
                .entry(name)
                .or_insert((a.ifa_flags, String::new(), Vec::new()));
            if !a.ifa_addr.is_null() {
                match unsafe { (*a.ifa_addr).sa_family as i32 } {
                    libc::AF_LINK => {
                        let link = unsafe { &*(a.ifa_addr as *const libc::sockaddr_dl) };
                        let n = link.sdl_nlen as usize;
                        let len = link.sdl_alen as usize;
                        // sockaddr_dl has a variable tail; long interface
                        // names can extend past libc's inline sdl_data array.
                        if len > 0 && 8 + n + len <= link.sdl_len as usize {
                            let bytes = unsafe {
                                std::slice::from_raw_parts(
                                    (a.ifa_addr as *const u8).add(8 + n),
                                    len,
                                )
                            };
                            e.1 = bytes
                                .iter()
                                .map(|b| format!("{b:02x}"))
                                .collect::<Vec<_>>()
                                .join(":");
                        }
                    }
                    libc::AF_INET => {
                        let addr = unsafe { &*(a.ifa_addr as *const libc::sockaddr_in) };
                        if !a.ifa_netmask.is_null() {
                            let mask = unsafe { &*(a.ifa_netmask as *const libc::sockaddr_in) };
                            let ip = Ipv4Addr::from(addr.sin_addr.s_addr.to_ne_bytes());
                            e.2.push(format!("{ip}/{}", mask.sin_addr.s_addr.count_ones()));
                        }
                    }
                    libc::AF_INET6 => {
                        let addr = unsafe { &*(a.ifa_addr as *const libc::sockaddr_in6) };
                        if !a.ifa_netmask.is_null() {
                            let mask = unsafe { &*(a.ifa_netmask as *const libc::sockaddr_in6) };
                            let mut bytes = addr.sin6_addr.s6_addr;
                            if bytes[0] == 0xfe && bytes[1] & 0xc0 == 0x80 {
                                bytes[2] = 0;
                                bytes[3] = 0;
                            }
                            e.2.push(format!(
                                "{}/{}",
                                Ipv6Addr::from(bytes),
                                mask.sin6_addr
                                    .s6_addr
                                    .iter()
                                    .map(|b| b.count_ones())
                                    .sum::<u32>()
                            ));
                        }
                    }
                    _ => {}
                }
            }
            p = a.ifa_next;
        }
        map.into_iter()
            .map(|(name, (flags, mac, addresses))| {
                let mut pairs = vec![("mac", mac)];
                for (key, flag) in [
                    ("is-up", libc::IFF_UP),
                    ("is-broadcast", libc::IFF_BROADCAST),
                    ("is-multicast", libc::IFF_MULTICAST),
                    ("is-loopback", libc::IFF_LOOPBACK),
                    ("is-point-to-point", libc::IFF_POINTOPOINT),
                ] {
                    pairs.push((key, (flags & flag as u32 != 0).to_string()));
                }
                pairs.push(("addresses", addresses.join(",")));
                item("net", &name, pairs)
            })
            .collect()
    }
    pub(super) fn system() -> Vec<ServerInfoItem> {
        // Darwin has no /proc/sys. A nonzero sysctl exit discards even partial
        // stdout, exactly as CombinedOutput's error branch in sysutil.
        output("sysctl", &["-a"])
            .map(|text| {
                vec![ServerInfoItem {
                    tp: "system".into(),
                    name: "sysctl".into(),
                    pairs: system_pairs(&text),
                }]
            })
            .unwrap_or_default()
    }
    pub(super) fn load() -> Vec<ServerInfoItem> {
        let mut result = Vec::new();
        // Darwin loadavg ABI: three fixed point u32 values, aligned long scale.
        #[repr(C)]
        #[derive(Clone, Copy, Default)]
        struct Load {
            values: [u32; 3],
            scale: libc::c_long,
        }
        if let Some(load) = sysctl::<Load>("vm.loadavg") {
            result.push(item(
                "cpu",
                "cpu",
                ["load1", "load5", "load15"]
                    .into_iter()
                    .zip(load.values)
                    .map(|(k, v)| (k, format!("{:.2}", v as f64 / load.scale as f64)))
                    .collect(),
            ));
        }
        // CPU times and disk IO are unavailable in the pinned Darwin !cgo
        // gopsutil collector; no fabricated usage/rate entries are emitted.
        if let Some((total, free)) = virtual_memory() {
            result.push(memory("virtual", total, total.wrapping_sub(free), free));
        }
        #[repr(C)]
        #[derive(Clone, Copy, Default)]
        struct Swap {
            total: u64,
            free: u64,
            used: u64,
            page: u32,
            encrypted: u32,
        }
        if let Some(s) = sysctl::<Swap>("vm.swapusage") {
            result.push(memory("swap", s.total, s.used, s.free));
        }
        if let Some(text) = output("netstat", &["-ibdnW"]) {
            result.extend(net_load(&text));
        }
        result
    }
    fn memory(name: &str, total: u64, used: u64, free: u64) -> ServerInfoItem {
        let percent = |n| {
            if total == 0 {
                "NaN".into()
            } else {
                format!("{:.2}", n as f64 / total as f64)
            }
        };
        item(
            "memory",
            name,
            vec![
                ("total", total.to_string()),
                ("used", used.to_string()),
                ("free", free.to_string()),
                ("used-percent", percent(used)),
                (
                    "free-percent",
                    if name == "virtual" {
                        format!("{:.2}", 1.0 - used as f64 / total as f64)
                    } else {
                        percent(free)
                    },
                ),
            ],
        )
    }
    fn net_load(text: &str) -> Vec<ServerInfoItem> {
        // A malformed row invalidates the entire gopsutil snapshot, including
        // rows for IP addresses that are not ultimately returned.
        let rows: Option<Vec<Option<ServerInfoItem>>> = text
            .lines()
            .skip(1)
            .map(|line| {
                let f: Vec<_> = line.split_whitespace().collect();
                if !(11..=13).contains(&f.len()) {
                    return None;
                }
                let start = if f.len() >= 12 { 4 } else { 3 };
                let value = |i: usize| -> Option<String> {
                    let v = *f.get(start + i)?;
                    Some(if v == "-" {
                        "0".into()
                    } else {
                        v.parse::<u64>().ok()?.to_string()
                    })
                };
                let values: Vec<_> = (0..6).map(value).collect::<Option<_>>()?;
                let drop = if f.len() == 12 { value(7)? } else { "0".into() };
                let link = f[2]
                    .strip_prefix("<Link#")
                    .and_then(|s| s.strip_suffix('>'));
                let Some(link) =
                    link.filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                else {
                    return Some(None);
                };
                link.parse::<u64>().ok()?;
                Some(Some(item(
                    "net",
                    f[0].trim_matches('*'),
                    vec![
                        ("bytes-ent", values[5].clone()),
                        ("bytes-recv", values[2].clone()),
                        ("packets-sent", values[3].clone()),
                        ("packets-recv", values[0].clone()),
                        ("errin", values[1].clone()),
                        ("errout", values[4].clone()),
                        ("dropin", "0".into()),
                        ("dropout", drop),
                        ("fifoin", "0".into()),
                        ("fifoout", "0".into()),
                    ],
                )))
            })
            .collect();
        rows.unwrap_or_default().into_iter().flatten().collect()
    }
}
#[cfg(test)]
#[path = "diagnostics_info_test.rs"]
mod tests;
