// Copyright 2026 AsterSQL.
use crate::*;

const GO_ORACLE: &str = concat!(
    "recovering\t0\t\t\t/pd/0/cluster/markers/snapshot-recovering\n",
    "pitr\t0\t\t\t/pd/0/cluster/markers/pitr-restore-mode\n",
    "maintenance_prefix\t0\t\t\t/pd/0/maintenance/\n",
    "service\t0\t\t\t/ms/0//registry/\n",
    "maintenance\t0\t\t\t/pd/0/maintenance/\n",
    "registry\t0\t\t\t/ms/0//registry/\n",
    "registry\t0\t\t127.0.0.1:2379\t/ms/0//registry/127.0.0.1:2379\n",
    "registry\t0\t\thttp://[::1]:2380/a/../\t/ms/0//registry/http://[::1]:2380/a/../\n",
    "registry\t0\t\t/a//../b/\t/ms/0//registry//a//../b/\n",
    "registry\t0\t\t服务\t/ms/0//registry/服务\n",
    "service\t0\ttso\t\t/ms/0/tso/registry/\n",
    "maintenance\t0\ttso\t\t/pd/0/maintenance/tso\n",
    "registry\t0\ttso\t\t/ms/0/tso/registry/\n",
    "registry\t0\ttso\t127.0.0.1:2379\t/ms/0/tso/registry/127.0.0.1:2379\n",
    "registry\t0\ttso\thttp://[::1]:2380/a/../\t/ms/0/tso/registry/http://[::1]:2380/a/../\n",
    "registry\t0\ttso\t/a//../b/\t/ms/0/tso/registry//a//../b/\n",
    "registry\t0\ttso\t服务\t/ms/0/tso/registry/服务\n",
    "service\t0\tscheduling\t\t/ms/0/scheduling/registry/\n",
    "maintenance\t0\tscheduling\t\t/pd/0/maintenance/scheduling\n",
    "registry\t0\tscheduling\t\t/ms/0/scheduling/registry/\n",
    "registry\t0\tscheduling\t127.0.0.1:2379\t/ms/0/scheduling/registry/127.0.0.1:2379\n",
    "registry\t0\tscheduling\thttp://[::1]:2380/a/../\t/ms/0/scheduling/registry/http://[::1]:2380/a/../\n",
    "registry\t0\tscheduling\t/a//../b/\t/ms/0/scheduling/registry//a//../b/\n",
    "registry\t0\tscheduling\t服务\t/ms/0/scheduling/registry/服务\n",
    "service\t0\ta/b\t\t/ms/0/a/b/registry/\n",
    "maintenance\t0\ta/b\t\t/pd/0/maintenance/a/b\n",
    "registry\t0\ta/b\t\t/ms/0/a/b/registry/\n",
    "registry\t0\ta/b\t127.0.0.1:2379\t/ms/0/a/b/registry/127.0.0.1:2379\n",
    "registry\t0\ta/b\thttp://[::1]:2380/a/../\t/ms/0/a/b/registry/http://[::1]:2380/a/../\n",
    "registry\t0\ta/b\t/a//../b/\t/ms/0/a/b/registry//a//../b/\n",
    "registry\t0\ta/b\t服务\t/ms/0/a/b/registry/服务\n",
    "service\t0\t/a//../b/\t\t/ms/0//a//../b//registry/\n",
    "maintenance\t0\t/a//../b/\t\t/pd/0/maintenance//a//../b/\n",
    "registry\t0\t/a//../b/\t\t/ms/0//a//../b//registry/\n",
    "registry\t0\t/a//../b/\t127.0.0.1:2379\t/ms/0//a//../b//registry/127.0.0.1:2379\n",
    "registry\t0\t/a//../b/\thttp://[::1]:2380/a/../\t/ms/0//a//../b//registry/http://[::1]:2380/a/../\n",
    "registry\t0\t/a//../b/\t/a//../b/\t/ms/0//a//../b//registry//a//../b/\n",
    "registry\t0\t/a//../b/\t服务\t/ms/0//a//../b//registry/服务\n",
    "service\t0\t.\t\t/ms/0/./registry/\n",
    "maintenance\t0\t.\t\t/pd/0/maintenance/.\n",
    "registry\t0\t.\t\t/ms/0/./registry/\n",
    "registry\t0\t.\t127.0.0.1:2379\t/ms/0/./registry/127.0.0.1:2379\n",
    "registry\t0\t.\thttp://[::1]:2380/a/../\t/ms/0/./registry/http://[::1]:2380/a/../\n",
    "registry\t0\t.\t/a//../b/\t/ms/0/./registry//a//../b/\n",
    "registry\t0\t.\t服务\t/ms/0/./registry/服务\n",
    "service\t0\t..\t\t/ms/0/../registry/\n",
    "maintenance\t0\t..\t\t/pd/0/maintenance/..\n",
    "registry\t0\t..\t\t/ms/0/../registry/\n",
    "registry\t0\t..\t127.0.0.1:2379\t/ms/0/../registry/127.0.0.1:2379\n",
    "registry\t0\t..\thttp://[::1]:2380/a/../\t/ms/0/../registry/http://[::1]:2380/a/../\n",
    "registry\t0\t..\t/a//../b/\t/ms/0/../registry//a//../b/\n",
    "registry\t0\t..\t服务\t/ms/0/../registry/服务\n",
    "service\t0\t维护\t\t/ms/0/维护/registry/\n",
    "maintenance\t0\t维护\t\t/pd/0/maintenance/维护\n",
    "registry\t0\t维护\t\t/ms/0/维护/registry/\n",
    "registry\t0\t维护\t127.0.0.1:2379\t/ms/0/维护/registry/127.0.0.1:2379\n",
    "registry\t0\t维护\thttp://[::1]:2380/a/../\t/ms/0/维护/registry/http://[::1]:2380/a/../\n",
    "registry\t0\t维护\t/a//../b/\t/ms/0/维护/registry//a//../b/\n",
    "registry\t0\t维护\t服务\t/ms/0/维护/registry/服务\n",
    "recovering\t42\t\t\t/pd/42/cluster/markers/snapshot-recovering\n",
    "pitr\t42\t\t\t/pd/42/cluster/markers/pitr-restore-mode\n",
    "maintenance_prefix\t42\t\t\t/pd/42/maintenance/\n",
    "service\t42\t\t\t/ms/42//registry/\n",
    "maintenance\t42\t\t\t/pd/42/maintenance/\n",
    "registry\t42\t\t\t/ms/42//registry/\n",
    "registry\t42\t\t127.0.0.1:2379\t/ms/42//registry/127.0.0.1:2379\n",
    "registry\t42\t\thttp://[::1]:2380/a/../\t/ms/42//registry/http://[::1]:2380/a/../\n",
    "registry\t42\t\t/a//../b/\t/ms/42//registry//a//../b/\n",
    "registry\t42\t\t服务\t/ms/42//registry/服务\n",
    "service\t42\ttso\t\t/ms/42/tso/registry/\n",
    "maintenance\t42\ttso\t\t/pd/42/maintenance/tso\n",
    "registry\t42\ttso\t\t/ms/42/tso/registry/\n",
    "registry\t42\ttso\t127.0.0.1:2379\t/ms/42/tso/registry/127.0.0.1:2379\n",
    "registry\t42\ttso\thttp://[::1]:2380/a/../\t/ms/42/tso/registry/http://[::1]:2380/a/../\n",
    "registry\t42\ttso\t/a//../b/\t/ms/42/tso/registry//a//../b/\n",
    "registry\t42\ttso\t服务\t/ms/42/tso/registry/服务\n",
    "service\t42\tscheduling\t\t/ms/42/scheduling/registry/\n",
    "maintenance\t42\tscheduling\t\t/pd/42/maintenance/scheduling\n",
    "registry\t42\tscheduling\t\t/ms/42/scheduling/registry/\n",
    "registry\t42\tscheduling\t127.0.0.1:2379\t/ms/42/scheduling/registry/127.0.0.1:2379\n",
    "registry\t42\tscheduling\thttp://[::1]:2380/a/../\t/ms/42/scheduling/registry/http://[::1]:2380/a/../\n",
    "registry\t42\tscheduling\t/a//../b/\t/ms/42/scheduling/registry//a//../b/\n",
    "registry\t42\tscheduling\t服务\t/ms/42/scheduling/registry/服务\n",
    "service\t42\ta/b\t\t/ms/42/a/b/registry/\n",
    "maintenance\t42\ta/b\t\t/pd/42/maintenance/a/b\n",
    "registry\t42\ta/b\t\t/ms/42/a/b/registry/\n",
    "registry\t42\ta/b\t127.0.0.1:2379\t/ms/42/a/b/registry/127.0.0.1:2379\n",
    "registry\t42\ta/b\thttp://[::1]:2380/a/../\t/ms/42/a/b/registry/http://[::1]:2380/a/../\n",
    "registry\t42\ta/b\t/a//../b/\t/ms/42/a/b/registry//a//../b/\n",
    "registry\t42\ta/b\t服务\t/ms/42/a/b/registry/服务\n",
    "service\t42\t/a//../b/\t\t/ms/42//a//../b//registry/\n",
    "maintenance\t42\t/a//../b/\t\t/pd/42/maintenance//a//../b/\n",
    "registry\t42\t/a//../b/\t\t/ms/42//a//../b//registry/\n",
    "registry\t42\t/a//../b/\t127.0.0.1:2379\t/ms/42//a//../b//registry/127.0.0.1:2379\n",
    "registry\t42\t/a//../b/\thttp://[::1]:2380/a/../\t/ms/42//a//../b//registry/http://[::1]:2380/a/../\n",
    "registry\t42\t/a//../b/\t/a//../b/\t/ms/42//a//../b//registry//a//../b/\n",
    "registry\t42\t/a//../b/\t服务\t/ms/42//a//../b//registry/服务\n",
    "service\t42\t.\t\t/ms/42/./registry/\n",
    "maintenance\t42\t.\t\t/pd/42/maintenance/.\n",
    "registry\t42\t.\t\t/ms/42/./registry/\n",
    "registry\t42\t.\t127.0.0.1:2379\t/ms/42/./registry/127.0.0.1:2379\n",
    "registry\t42\t.\thttp://[::1]:2380/a/../\t/ms/42/./registry/http://[::1]:2380/a/../\n",
    "registry\t42\t.\t/a//../b/\t/ms/42/./registry//a//../b/\n",
    "registry\t42\t.\t服务\t/ms/42/./registry/服务\n",
    "service\t42\t..\t\t/ms/42/../registry/\n",
    "maintenance\t42\t..\t\t/pd/42/maintenance/..\n",
    "registry\t42\t..\t\t/ms/42/../registry/\n",
    "registry\t42\t..\t127.0.0.1:2379\t/ms/42/../registry/127.0.0.1:2379\n",
    "registry\t42\t..\thttp://[::1]:2380/a/../\t/ms/42/../registry/http://[::1]:2380/a/../\n",
    "registry\t42\t..\t/a//../b/\t/ms/42/../registry//a//../b/\n",
    "registry\t42\t..\t服务\t/ms/42/../registry/服务\n",
    "service\t42\t维护\t\t/ms/42/维护/registry/\n",
    "maintenance\t42\t维护\t\t/pd/42/maintenance/维护\n",
    "registry\t42\t维护\t\t/ms/42/维护/registry/\n",
    "registry\t42\t维护\t127.0.0.1:2379\t/ms/42/维护/registry/127.0.0.1:2379\n",
    "registry\t42\t维护\thttp://[::1]:2380/a/../\t/ms/42/维护/registry/http://[::1]:2380/a/../\n",
    "registry\t42\t维护\t/a//../b/\t/ms/42/维护/registry//a//../b/\n",
    "registry\t42\t维护\t服务\t/ms/42/维护/registry/服务\n",
    "recovering\t18446744073709551615\t\t\t/pd/18446744073709551615/cluster/markers/snapshot-recovering\n",
    "pitr\t18446744073709551615\t\t\t/pd/18446744073709551615/cluster/markers/pitr-restore-mode\n",
    "maintenance_prefix\t18446744073709551615\t\t\t/pd/18446744073709551615/maintenance/\n",
    "service\t18446744073709551615\t\t\t/ms/18446744073709551615//registry/\n",
    "maintenance\t18446744073709551615\t\t\t/pd/18446744073709551615/maintenance/\n",
    "registry\t18446744073709551615\t\t\t/ms/18446744073709551615//registry/\n",
    "registry\t18446744073709551615\t\t127.0.0.1:2379\t/ms/18446744073709551615//registry/127.0.0.1:2379\n",
    "registry\t18446744073709551615\t\thttp://[::1]:2380/a/../\t/ms/18446744073709551615//registry/http://[::1]:2380/a/../\n",
    "registry\t18446744073709551615\t\t/a//../b/\t/ms/18446744073709551615//registry//a//../b/\n",
    "registry\t18446744073709551615\t\t服务\t/ms/18446744073709551615//registry/服务\n",
    "service\t18446744073709551615\ttso\t\t/ms/18446744073709551615/tso/registry/\n",
    "maintenance\t18446744073709551615\ttso\t\t/pd/18446744073709551615/maintenance/tso\n",
    "registry\t18446744073709551615\ttso\t\t/ms/18446744073709551615/tso/registry/\n",
    "registry\t18446744073709551615\ttso\t127.0.0.1:2379\t/ms/18446744073709551615/tso/registry/127.0.0.1:2379\n",
    "registry\t18446744073709551615\ttso\thttp://[::1]:2380/a/../\t/ms/18446744073709551615/tso/registry/http://[::1]:2380/a/../\n",
    "registry\t18446744073709551615\ttso\t/a//../b/\t/ms/18446744073709551615/tso/registry//a//../b/\n",
    "registry\t18446744073709551615\ttso\t服务\t/ms/18446744073709551615/tso/registry/服务\n",
    "service\t18446744073709551615\tscheduling\t\t/ms/18446744073709551615/scheduling/registry/\n",
    "maintenance\t18446744073709551615\tscheduling\t\t/pd/18446744073709551615/maintenance/scheduling\n",
    "registry\t18446744073709551615\tscheduling\t\t/ms/18446744073709551615/scheduling/registry/\n",
    "registry\t18446744073709551615\tscheduling\t127.0.0.1:2379\t/ms/18446744073709551615/scheduling/registry/127.0.0.1:2379\n",
    "registry\t18446744073709551615\tscheduling\thttp://[::1]:2380/a/../\t/ms/18446744073709551615/scheduling/registry/http://[::1]:2380/a/../\n",
    "registry\t18446744073709551615\tscheduling\t/a//../b/\t/ms/18446744073709551615/scheduling/registry//a//../b/\n",
    "registry\t18446744073709551615\tscheduling\t服务\t/ms/18446744073709551615/scheduling/registry/服务\n",
    "service\t18446744073709551615\ta/b\t\t/ms/18446744073709551615/a/b/registry/\n",
    "maintenance\t18446744073709551615\ta/b\t\t/pd/18446744073709551615/maintenance/a/b\n",
    "registry\t18446744073709551615\ta/b\t\t/ms/18446744073709551615/a/b/registry/\n",
    "registry\t18446744073709551615\ta/b\t127.0.0.1:2379\t/ms/18446744073709551615/a/b/registry/127.0.0.1:2379\n",
    "registry\t18446744073709551615\ta/b\thttp://[::1]:2380/a/../\t/ms/18446744073709551615/a/b/registry/http://[::1]:2380/a/../\n",
    "registry\t18446744073709551615\ta/b\t/a//../b/\t/ms/18446744073709551615/a/b/registry//a//../b/\n",
    "registry\t18446744073709551615\ta/b\t服务\t/ms/18446744073709551615/a/b/registry/服务\n",
    "service\t18446744073709551615\t/a//../b/\t\t/ms/18446744073709551615//a//../b//registry/\n",
    "maintenance\t18446744073709551615\t/a//../b/\t\t/pd/18446744073709551615/maintenance//a//../b/\n",
    "registry\t18446744073709551615\t/a//../b/\t\t/ms/18446744073709551615//a//../b//registry/\n",
    "registry\t18446744073709551615\t/a//../b/\t127.0.0.1:2379\t/ms/18446744073709551615//a//../b//registry/127.0.0.1:2379\n",
    "registry\t18446744073709551615\t/a//../b/\thttp://[::1]:2380/a/../\t/ms/18446744073709551615//a//../b//registry/http://[::1]:2380/a/../\n",
    "registry\t18446744073709551615\t/a//../b/\t/a//../b/\t/ms/18446744073709551615//a//../b//registry//a//../b/\n",
    "registry\t18446744073709551615\t/a//../b/\t服务\t/ms/18446744073709551615//a//../b//registry/服务\n",
    "service\t18446744073709551615\t.\t\t/ms/18446744073709551615/./registry/\n",
    "maintenance\t18446744073709551615\t.\t\t/pd/18446744073709551615/maintenance/.\n",
    "registry\t18446744073709551615\t.\t\t/ms/18446744073709551615/./registry/\n",
    "registry\t18446744073709551615\t.\t127.0.0.1:2379\t/ms/18446744073709551615/./registry/127.0.0.1:2379\n",
    "registry\t18446744073709551615\t.\thttp://[::1]:2380/a/../\t/ms/18446744073709551615/./registry/http://[::1]:2380/a/../\n",
    "registry\t18446744073709551615\t.\t/a//../b/\t/ms/18446744073709551615/./registry//a//../b/\n",
    "registry\t18446744073709551615\t.\t服务\t/ms/18446744073709551615/./registry/服务\n",
    "service\t18446744073709551615\t..\t\t/ms/18446744073709551615/../registry/\n",
    "maintenance\t18446744073709551615\t..\t\t/pd/18446744073709551615/maintenance/..\n",
    "registry\t18446744073709551615\t..\t\t/ms/18446744073709551615/../registry/\n",
    "registry\t18446744073709551615\t..\t127.0.0.1:2379\t/ms/18446744073709551615/../registry/127.0.0.1:2379\n",
    "registry\t18446744073709551615\t..\thttp://[::1]:2380/a/../\t/ms/18446744073709551615/../registry/http://[::1]:2380/a/../\n",
    "registry\t18446744073709551615\t..\t/a//../b/\t/ms/18446744073709551615/../registry//a//../b/\n",
    "registry\t18446744073709551615\t..\t服务\t/ms/18446744073709551615/../registry/服务\n",
    "service\t18446744073709551615\t维护\t\t/ms/18446744073709551615/维护/registry/\n",
    "maintenance\t18446744073709551615\t维护\t\t/pd/18446744073709551615/maintenance/维护\n",
    "registry\t18446744073709551615\t维护\t\t/ms/18446744073709551615/维护/registry/\n",
    "registry\t18446744073709551615\t维护\t127.0.0.1:2379\t/ms/18446744073709551615/维护/registry/127.0.0.1:2379\n",
    "registry\t18446744073709551615\t维护\thttp://[::1]:2380/a/../\t/ms/18446744073709551615/维护/registry/http://[::1]:2380/a/../\n",
    "registry\t18446744073709551615\t维护\t/a//../b/\t/ms/18446744073709551615/维护/registry//a//../b/\n",
    "registry\t18446744073709551615\t维护\t服务\t/ms/18446744073709551615/维护/registry/服务\n",
    "prefix\t0\t\t\t.\n",
    "prefix\t0\t/\t\t/\n",
    "prefix\t0\ttso\t\t.\n",
    "prefix\t0\ta/b\t\ta\n",
    "prefix\t0\ta/../b\t\t.\n",
    "prefix\t0\t/a//../b/\t\t/b\n",
    "prefix\t0\t../a/\t\t../a\n",
    "prefix\t0\t/ms/42/tso/registry/\t\t/ms/42/tso/registry\n",
    "prefix\t0\t/ms/42/tso/registry/http://[::1]:2380/a/../\t\t/ms/42/tso/registry/http:/[::1]:2380\n",
);

#[test]
fn registry_marker_paths_go_bytes() {
    let mut count = 0;
    for line in GO_ORACLE.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 5);
        let paths = ClusterPaths::new(fields[1].parse().unwrap());
        let value = match fields[0] {
            "recovering" => paths.recovering_mark_path(),
            "pitr" => paths.pitr_restore_mode_mark_path(),
            "maintenance_prefix" => paths.maintenance_task_path_prefix(),
            "maintenance" => paths.maintenance_task_path(fields[2]),
            "service" => paths.service_path(fields[2]),
            "registry" => paths.registry_path(fields[2], fields[3]),
            "prefix" => prefix(fields[2]),
            other => panic!("unknown oracle operation: {other}"),
        };
        assert_eq!(value.as_bytes(), fields[4].as_bytes(), "{line}");
        println!(
            "ORACLE\t{}\t{}\t{}\t{}\t{value}",
            fields[0], fields[1], fields[2], fields[3]
        );
        count += 1;
    }
    assert_eq!(count, 186);
}

#[test]
fn registry_marker_paths_boundaries_and_isolation() {
    let paths = ClusterPaths::new(42);
    let other = ClusterPaths::new(43);
    assert_eq!(paths.service_path("tso"), "/ms/42/tso/registry/");
    assert_eq!(paths.registry_path("tso", ""), paths.service_path("tso"));
    assert_eq!(
        paths.maintenance_task_path(""),
        paths.maintenance_task_path_prefix()
    );
    for name in ["", "tso", "/a//../b/", "维护"] {
        assert_ne!(paths.service_path(name), other.service_path(name));
        assert_ne!(
            paths.maintenance_task_path(name),
            other.maintenance_task_path(name)
        );
        for addr in ["", "http://[::1]:2379", "/a//../b/"] {
            let key = paths.registry_path(name, addr);
            assert!(key.starts_with(&paths.service_path(name)));
            assert_ne!(key, other.registry_path(name, addr));
            assert_eq!(&key[paths.service_path(name).len()..], addr);
        }
    }
    assert!(
        !paths
            .registry_path("tso2", "host")
            .starts_with(&paths.service_path("tso"))
    );
    assert!(
        !paths
            .maintenance_task_path("a")
            .starts_with(&other.maintenance_task_path_prefix())
    );
    assert_ne!(paths.recovering_mark_path(), other.recovering_mark_path());
    assert_ne!(
        paths.pitr_restore_mode_mark_path(),
        other.pitr_restore_mode_mark_path()
    );
    assert_ne!(
        paths.recovering_mark_path(),
        paths.pitr_restore_mode_mark_path()
    );
}
