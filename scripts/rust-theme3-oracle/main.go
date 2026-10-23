// Copyright 2026 AsterSQL.
// Command rust-theme3-oracle records real Go behavior for Rust compatibility tests.
package main

import "os"

func main() {
	if len(os.Args) != 2 {
		panic("usage: rust-theme3-oracle <scenario>")
	}
	switch os.Args[1] {
	case "cluster_paths":
		clusterPaths()
	case "member_config_paths":
		memberConfigPaths()
	default:
		panic("unknown scenario")
	}
}
