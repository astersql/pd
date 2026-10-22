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
)

func TestRustCoreTask05Oracle(t *testing.T) {
	var scenarios []struct {
		Name       string
		Capacity   int64
		Operations [][]json.RawMessage
		Expected   []struct {
			Capacity  int64
			Used      int64
			Count     uint
			Available bool
			Result    int64
		}
	}
	data, err := os.ReadFile("rust-fixtures/core_task_05.json")
	require.NoError(t, err)
	require.NoError(t, json.Unmarshal(data, &scenarios))
	for _, s := range scenarios {
		t.Run(s.Name, func(t *testing.T) {
			w := newWindow(s.Capacity)
			for i, op := range s.Operations {
				var action string
				var value int64
				require.NoError(t, json.Unmarshal(op[0], &action))
				require.NoError(t, json.Unmarshal(op[1], &value))
				result := int64(0)
				switch action {
				case "take":
					if w.take(value) {
						result = 1
					}
				case "ack":
					result = w.ack(value)
				case "reset":
					w.reset(value)
				default:
					t.Fatalf("unknown action %q", action)
				}
				t.Logf("%d: capacity=%d used=%d count=%d available=%t result=%d", i, w.capacity, w.getUsed(), w.count, w.available(), result)
				require.Len(t, s.Expected, len(s.Operations))
				if len(s.Expected) > 0 {
					e := s.Expected[i]
					require.Equal(t, e.Capacity, w.capacity)
					require.Equal(t, e.Used, w.getUsed())
					require.Equal(t, e.Count, w.count)
					require.Equal(t, e.Available, w.available())
					require.Equal(t, e.Result, result)
				}
			}
		})
	}
}
