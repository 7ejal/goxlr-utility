// Collect the six bounded read-only ARM metrics after an authorized 910 update.
// These counters are not DSP utilization, clock or overrun measurements.
// Run: go run tools/collect_goxlr_arm_diagnostics.go -output arm-metrics.json
package main

import (
	"bytes"
	"encoding/json"
	"flag"
	"fmt"
	"net/http"
	"os"
	"time"
)

var armMetrics = []string{"CpuAudioBit22Count", "CpuAudioBit23Count", "EffectRingCapacity", "EffectRingBuffer", "EffectRingUsed", "SchedulerTickLow"}

type diagnosticSample struct {
	HostElapsedMS float64
	RequestMS     float64
	Values        map[string]uint32
}

type diagnosticReport struct {
	Firmware string
	Units    map[string]string
	Samples  []diagnosticSample
	Error    string `json:",omitempty"`
}

type diagnosticAPI struct {
	endpoint string
	client   *http.Client
}

func (api diagnosticAPI) request(body any, result any) error {
	data, err := json.Marshal(body)
	if err != nil {
		return err
	}
	response, err := api.client.Post(api.endpoint, "application/json", bytes.NewReader(data))
	if err != nil {
		return err
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		return fmt.Errorf("daemon HTTP status %d", response.StatusCode)
	}
	return json.NewDecoder(response.Body).Decode(result)
}

func (api diagnosticAPI) device() (string, error) {
	var reply struct {
		Error  string
		Status struct {
			Mixers map[string]struct {
				Hardware struct {
					DeviceType string `json:"device_type"`
					Versions   struct{ Firmware []uint32 }
				}
			}
		}
	}
	if err := api.request("GetStatus", &reply); err != nil {
		return "", err
	}
	if reply.Error != "" {
		return "", fmt.Errorf("%s", reply.Error)
	}
	if len(reply.Status.Mixers) != 1 {
		return "", fmt.Errorf("expected exactly one full-size GoXLR on 910")
	}
	for serial, device := range reply.Status.Mixers {
		if device.Hardware.DeviceType != "Full" || fmt.Sprint(device.Hardware.Versions.Firmware) != "[1 5 1 910]" {
			return "", fmt.Errorf("ARM diagnostics requires exact firmware 1.5.1.910; no diagnostic request sent")
		}
		return serial, nil
	}
	return "", fmt.Errorf("GoXLR missing")
}

func (api diagnosticAPI) sample(serial string) (map[string]uint32, error) {
	values := make(map[string]uint32, len(armMetrics))
	for _, metric := range armMetrics {
		var reply struct {
			ArmDiagnostic *uint32
			Error         string
		}
		if err := api.request(map[string]any{"ReadArmDiagnostic": []string{serial, metric}}, &reply); err != nil {
			return values, err
		}
		if reply.Error != "" || reply.ArmDiagnostic == nil {
			return values, fmt.Errorf("%s: diagnostic unavailable (%s)", metric, reply.Error)
		}
		values[metric] = *reply.ArmDiagnostic
	}
	if values["EffectRingCapacity"] != 0x404 || values["EffectRingBuffer"] != 0x3d79d || values["EffectRingUsed"] >= 0x404 {
		return values, fmt.Errorf("effect ring did not match its validated layout")
	}
	return values, nil
}

func main() {
	endpoint := flag.String("endpoint", "http://127.0.0.1:14564/api/command", "local daemon command endpoint")
	output := flag.String("output", "", "new JSON report; existing files are never overwritten")
	count := flag.Int("samples", 20, "number of six-metric snapshots (1..1000)")
	interval := flag.Duration("interval", 250*time.Millisecond, "pause between snapshots (at least 100ms)")
	flag.Parse()
	if err := collectDiagnostics(*endpoint, *output, *count, *interval); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func collectDiagnostics(endpoint, output string, count int, interval time.Duration) error {
	if output == "" || count < 1 || count > 1000 || interval < 100*time.Millisecond {
		return fmt.Errorf("pass a new output path, 1..1000 samples and interval >=100ms")
	}
	api := diagnosticAPI{endpoint, &http.Client{Timeout: 3 * time.Second}}
	serial, err := api.device()
	if err != nil {
		return err
	}
	file, err := os.OpenFile(output, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0600)
	if err != nil {
		return err
	}
	defer file.Close()
	report := diagnosticReport{Firmware: "1.5.1.910", Units: map[string]string{
		"CpuAudioBit22Count": "original u32 event count; resets on CPU-audio reconfiguration",
		"CpuAudioBit23Count": "original u32 event count; not an overrun/underrun counter",
		"EffectRingCapacity": "bytes", "EffectRingBuffer": "ARM address", "EffectRingUsed": "bytes",
		"SchedulerTickLow": "wrapping native u32 ticks; tick duration uncalibrated",
		"HostElapsedMS":    "host monotonic elapsed milliseconds", "RequestMS": "host six-request duration; not DSP time",
	}}
	start := time.Now()
	for i := 0; i < count; i++ {
		before := time.Now()
		values, sampleErr := api.sample(serial)
		report.Samples = append(report.Samples, diagnosticSample{float64(time.Since(start).Microseconds()) / 1000, float64(time.Since(before).Microseconds()) / 1000, values})
		if sampleErr != nil {
			err = sampleErr
			report.Error = err.Error()
			break
		}
		if i+1 < count {
			time.Sleep(interval)
		}
	}
	encoder := json.NewEncoder(file)
	encoder.SetIndent("", "  ")
	if writeErr := encoder.Encode(report); writeErr != nil {
		return writeErr
	}
	return err
}
