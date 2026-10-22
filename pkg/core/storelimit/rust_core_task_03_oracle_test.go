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

func TestRustCoreTask03Oracle(t *testing.T) {
	var scenarios []struct {
		Name       string  `json:"name"`
		Rate       float64 `json:"rate"`
		Operations []struct {
			Action   string                 `json:"action"`
			Cost     int64                  `json:"cost"`
			Typ      Type                   `json:"typ"`
			Priority constant.PriorityLevel `json:"priority"`
		} `json:"operations"`
		Expected []bool `json:"expected"`
	}
	data, err := os.ReadFile("rust-fixtures/core_task_03.json")
	require.NoError(t, err)
	require.NoError(t, json.Unmarshal(data, &scenarios))
	for _, scenario := range scenarios {
		t.Run(scenario.Name, func(t *testing.T) {
			limiter := NewStoreRateLimit(scenario.Rate).(*StoreRateLimit)
			require.Equal(t, VersionV1, limiter.Version())
			for typ := Type(0); typ < storeLimitTypeLen; typ++ {
				require.Equal(t, scenario.Rate, limiter.Rate(typ))
			}
			actual := make([]bool, 0, len(scenario.Operations))
			for _, op := range scenario.Operations {
				switch op.Action {
				case "available":
					actual = append(actual, limiter.Available(op.Cost, op.Typ, op.Priority))
				case "take":
					actual = append(actual, limiter.Take(op.Cost, op.Typ, op.Priority))
				case "ack":
					limiter.Ack(op.Cost, op.Typ)
				case "feedback":
					limiter.Feedback(float64(op.Cost))
				default:
					t.Fatalf("unknown action %q", op.Action)
				}
			}
			t.Logf("Go oracle %s: %v", scenario.Name, actual)
			require.Equal(t, scenario.Expected, actual)
			require.Panics(t, func() { limiter.Take(1, Type(-1), constant.Low) })
			require.Panics(t, func() { limiter.Available(1, storeLimitTypeLen, constant.Low) })
		})
	}
}
