// Copyright 2026 AsterSQL.

// This command emits deterministic wire fixtures using the Go PD oracle.
package main

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"

	"github.com/gogo/protobuf/proto"

	"github.com/pingcap/kvproto/pkg/metapb"
	"github.com/pingcap/kvproto/pkg/pdpb"
)

func main() {
	header := &pdpb.RequestHeader{ClusterId: 42, SenderId: 7, CallerId: "oracle", CallerComponent: "pd"}
	fixtures := map[string]proto.Message{
		"request_header":   header,
		"region_heartbeat": &pdpb.RegionHeartbeatRequest{Header: header, Region: &metapb.Region{Id: 99, StartKey: []byte{0, 1}, EndKey: []byte{255}}, Leader: &metapb.Peer{Id: 3, StoreId: 4}, BytesWritten: 1234},
		"tso_request":      &pdpb.TsoRequest{Header: header, Count: 2, DcLocation: "global"},
		"tso_response":     &pdpb.TsoResponse{Header: &pdpb.ResponseHeader{ClusterId: 42}, Count: 2, Timestamp: &pdpb.Timestamp{Physical: 123456, Logical: 17, SuffixBits: 2}},
		"members_response": &pdpb.GetMembersResponse{Header: &pdpb.ResponseHeader{ClusterId: 42}, Members: []*pdpb.Member{{Name: "pd-1", MemberId: 9, ClientUrls: []string{"http://127.0.0.1:2379"}}}, Leader: &pdpb.Member{Name: "pd-1", MemberId: 9}},
		"keyspace_oneof":   &pdpb.GetGCSafePointV2Request{Header: header, Keyspace: &pdpb.GetGCSafePointV2Request_KeyspaceId{KeyspaceId: 0}},
		"error_enum":       &pdpb.ResponseHeader{ClusterId: 42, Error: &pdpb.Error{Type: pdpb.ErrorType_NOT_BOOTSTRAPPED, Message: "not bootstrapped"}},
	}
	out := make(map[string]string, len(fixtures))
	for name, message := range fixtures {
		wire, err := proto.Marshal(message)
		if err != nil {
			panic(err)
		}
		out[name] = hex.EncodeToString(wire)
	}
	data, err := json.MarshalIndent(out, "", "  ")
	if err != nil {
		panic(err)
	}
	if _, err := fmt.Fprintln(os.Stdout, string(data)); err != nil {
		panic(err)
	}
}
