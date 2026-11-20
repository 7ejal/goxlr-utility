package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"sync/atomic"
	"testing"
	"time"
)

func diagnosticServer(t *testing.T, build int, calls *atomic.Int32, corrupt bool) *httptest.Server {
	t.Helper()
	return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body json.RawMessage
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Error(err)
			return
		}
		if string(body) == `"GetStatus"` {
			json.NewEncoder(w).Encode(map[string]any{"Status": map[string]any{"mixers": map[string]any{
				"fixture": map[string]any{"hardware": map[string]any{"device_type": "Full", "versions": map[string]any{"firmware": []int{1, 5, 1, build}}}},
			}}})
			return
		}
		calls.Add(1)
		var request map[string][]string
		if err := json.Unmarshal(body, &request); err != nil || len(request["ReadArmDiagnostic"]) != 2 {
			t.Errorf("Wrong diagnostic request: %s", body)
			return
		}
		metric := request["ReadArmDiagnostic"][1]
		values := map[string]uint32{"CpuAudioBit22Count": 0xffffffff, "CpuAudioBit23Count": 3,
			"EffectRingCapacity": 0x404, "EffectRingBuffer": 0x3d79d, "EffectRingUsed": 12, "SchedulerTickLow": 0xffffffff}
		if corrupt && metric == "EffectRingBuffer" {
			json.NewEncoder(w).Encode(map[string]any{"ArmDiagnostic": -1})
			return
		}
		json.NewEncoder(w).Encode(map[string]uint32{"ArmDiagnostic": values[metric]})
	}))
}

func TestRejects909BeforeDiagnosticOrFileCreation(t *testing.T) {
	var calls atomic.Int32
	server := diagnosticServer(t, 909, &calls, false)
	defer server.Close()
	path := filepath.Join(t.TempDir(), "report.json")
	if collectDiagnostics(server.URL, path, 1, 100*time.Millisecond) == nil || calls.Load() != 0 {
		t.Fatal("Unsupported firmware was probed")
	}
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Fatal("Unsupported device created a report")
	}
}

func TestUnsignedReportAndOverwriteProtection(t *testing.T) {
	var calls atomic.Int32
	server := diagnosticServer(t, 910, &calls, false)
	defer server.Close()
	path := filepath.Join(t.TempDir(), "report.json")
	if err := collectDiagnostics(server.URL, path, 1, 100*time.Millisecond); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var report diagnosticReport
	if err := json.Unmarshal(data, &report); err != nil || len(report.Samples) != 1 || report.Samples[0].Values["CpuAudioBit22Count"] != 0xffffffff || calls.Load() != 6 {
		t.Fatalf("Lost unsigned snapshot: %s (%v)", data, err)
	}
	if collectDiagnostics(server.URL, path, 1, 100*time.Millisecond) == nil || calls.Load() != 6 {
		t.Fatal("Existing report was overwritten or new diagnostics sent")
	}
}

func TestMalformedHardwareReplyLeavesErrorReport(t *testing.T) {
	var calls atomic.Int32
	server := diagnosticServer(t, 910, &calls, true)
	defer server.Close()
	path := filepath.Join(t.TempDir(), "report.json")
	if collectDiagnostics(server.URL, path, 1, 100*time.Millisecond) == nil {
		t.Fatal("Accepted negative unsigned metric")
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var report diagnosticReport
	if json.Unmarshal(data, &report) != nil || report.Error == "" || calls.Load() != 4 {
		t.Fatalf("Missing bounded failure report: %s", data)
	}
}
