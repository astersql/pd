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

func TestRustCoreTask07Oracle(t *testing.T) {
	var ops []struct {
		Action   string
		Token    int64
		Error    float64
		Capacity int64
		Sum      float64
		Used     []int64
	}
	data, err := os.ReadFile("rust-fixtures/core_task_07.json")
	require.NoError(t, err)
	require.NoError(t, json.Unmarshal(data, &ops))
	s := NewSlidingWindows()
	for i, op := range ops {
		switch op.Action {
		case "take":
			require.True(t, s.Take(op.Token, SendSnapshot, constant.Low))
		case "feedback":
			s.Feedback(op.Error)
		default:
			t.Fatalf("unknown action %s", op.Action)
		}
		t.Logf("%d capacity=%d sum=%g used=%v", i, s.GetCap(), s.lastSum, s.GetUsed())
		require.Equal(t, op.Capacity, s.GetCap())
		require.Equal(t, op.Sum, s.lastSum)
		require.Equal(t, op.Used, s.GetUsed())
	}
}
