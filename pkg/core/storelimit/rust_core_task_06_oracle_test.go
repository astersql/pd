// Copyright 2026 TiKV Project Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

package storelimit

import (
	"encoding/json"
	"os"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/tikv/pd/pkg/core/constant"
)

func TestRustCoreTask06Oracle(t *testing.T) {
	var ops []struct {
		Action string
		Token  int64
		Typ    int
		Level  int
		Used   []int64
		Result bool
	}
	data, err := os.ReadFile("rust-fixtures/core_task_06.json")
	require.NoError(t, err)
	require.NoError(t, json.Unmarshal(data, &ops))
	s := NewSlidingWindows()
	require.Equal(t, "v2", s.Version())
	for i, op := range ops {
		result := false
		switch op.Action {
		case "take":
			result = s.Take(op.Token, Type(op.Typ), constant.PriorityLevel(op.Level))
		case "available":
			result = s.Available(op.Token, Type(op.Typ), constant.PriorityLevel(op.Level))
		case "ack":
			s.Ack(op.Token, Type(op.Typ))
		default:
			t.Fatalf("unknown action %s", op.Action)
		}
		t.Logf("%d result=%t used=%v", i, result, s.GetUsed())
		require.Len(t, op.Used, 4)
		require.Equal(t, op.Used, s.GetUsed())
		require.Equal(t, op.Result, result)
	}
}
