// Copyright 2026 AsterSQL.

// Test-only isolated etcd process. EOF closes the server; stop/start reuse its WAL.
package main

import (
	"bufio"
	"fmt"
	"net"
	"net/url"
	"os"
	"path/filepath"
	"time"

	"go.etcd.io/etcd/api/v3/version"
	"go.etcd.io/etcd/server/v3/embed"
)

func localURL() url.URL {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		panic(err)
	}
	address := listener.Addr().String()
	if err := listener.Close(); err != nil {
		panic(err)
	}
	return url.URL{Scheme: "http", Host: address}
}

func main() {
	if len(os.Args) != 2 {
		panic("expected fixture directory")
	}
	dir := os.Args[1]
	cfg := embed.NewConfig()
	cfg.Dir = filepath.Join(dir, "data")
	cfg.LogLevel = "error"
	cfg.LogOutputs = []string{filepath.Join(dir, "etcd.log")}
	cfg.ListenClientUrls = []url.URL{localURL()}
	cfg.AdvertiseClientUrls = cfg.ListenClientUrls
	cfg.ListenPeerUrls = []url.URL{localURL()}
	cfg.AdvertisePeerUrls = cfg.ListenPeerUrls
	cfg.InitialCluster = cfg.InitialClusterFromName(cfg.Name)
	var server *embed.Etcd
	defer func() {
		if server != nil {
			server.Close()
		}
	}()
	start := func() {
		var err error
		server, err = embed.StartEtcd(cfg)
		if err != nil {
			panic(err)
		}
		select {
		case <-server.Server.ReadyNotify():
		case <-time.After(15 * time.Second):
			panic("etcd readiness timeout")
		}
	}
	publish := func(name, value string) {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(value), 0600); err != nil {
			panic(err)
		}
	}
	start()
	publish("ready", cfg.AdvertiseClientUrls[0].String())
	fmt.Fprintf(os.Stderr, "isolated etcd %s ready at %s\n", version.Version, cfg.AdvertiseClientUrls[0].String())
	scanner := bufio.NewScanner(os.Stdin)
	for scanner.Scan() {
		switch scanner.Text() {
		case "stop":
			if server != nil {
				server.Close()
				server = nil
			}
			publish("stopped", "ok")
		case "start":
			if server == nil {
				start()
			}
			publish("restarted", "ok")
		default:
			panic("unknown fixture command")
		}
	}
	if err := scanner.Err(); err != nil {
		panic(err)
	}
}
