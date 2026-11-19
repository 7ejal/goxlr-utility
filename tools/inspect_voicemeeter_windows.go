//go:build windows

// Read the running Voicemeeter routing through its official Remote API. No setters or launch calls.
// Run: go run tools/inspect_voicemeeter_windows.go
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"math"
	"os"
	"syscall"
	"time"
	"unsafe"
)

func main() {
	path := flag.String("dll", `C:\Program Files (x86)\VB\Voicemeeter\VoicemeeterRemote64.dll`, "installed 64-bit Remote API DLL")
	flag.Parse()
	if err := inspect(*path); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func inspect(path string) error {
	dll, err := syscall.LoadDLL(path)
	if err != nil {
		return err
	}
	defer dll.Release()
	procedures := map[string]*syscall.Proc{}
	for _, name := range []string{"VBVMR_Login", "VBVMR_Logout", "VBVMR_IsParametersDirty", "VBVMR_GetParameterFloat", "VBVMR_GetParameterStringA"} {
		p, err := dll.FindProc(name)
		if err != nil {
			return err
		}
		procedures[name] = p
	}
	result, _, _ := procedures["VBVMR_Login"].Call()
	if int32(result) < 0 {
		return fmt.Errorf("Voicemeeter login returned %d", int32(result))
	}
	defer procedures["VBVMR_Logout"].Call()
	if result != 0 {
		return fmt.Errorf("Voicemeeter is not running; no application was launched")
	}
	time.Sleep(100 * time.Millisecond)
	procedures["VBVMR_IsParametersDirty"].Call()
	readFloat := func(name string) (float32, error) {
		key, _ := syscall.BytePtrFromString(name)
		var bits uint32
		r, _, _ := procedures["VBVMR_GetParameterFloat"].Call(uintptr(unsafe.Pointer(key)), uintptr(unsafe.Pointer(&bits)))
		if int32(r) != 0 {
			return 0, fmt.Errorf("%s returned %d", name, int32(r))
		}
		return math.Float32frombits(bits), nil
	}
	readText := func(name string) string {
		key, _ := syscall.BytePtrFromString(name)
		buffer := make([]byte, 512)
		r, _, _ := procedures["VBVMR_GetParameterStringA"].Call(uintptr(unsafe.Pointer(key)), uintptr(unsafe.Pointer(&buffer[0])))
		if int32(r) != 0 {
			return "unavailable"
		}
		for i, v := range buffer {
			if v == 0 {
				return string(buffer[:i])
			}
		}
		return "invalid unterminated string"
	}
	var strips, buses []map[string]any
	for i := 0; i < 8; i++ {
		prefix := fmt.Sprintf("Strip[%d].", i)
		row := map[string]any{"index": i, "label": readText(prefix + "Label")}
		if i < 5 {
			row["device"] = readText(prefix + "device.name")
		}
		for _, name := range []string{"Mute", "Gain", "A1", "A2", "A3", "A4", "A5", "B1", "B2", "B3"} {
			value, err := readFloat(prefix + name)
			if err != nil {
				return err
			}
			row[name] = value
		}
		strips = append(strips, row)
	}
	for i := 0; i < 8; i++ {
		prefix := fmt.Sprintf("Bus[%d].", i)
		row := map[string]any{"index": i, "label": readText(prefix + "Label")}
		if i < 5 {
			row["device"] = readText(prefix + "device.name")
		}
		for _, name := range []string{"Mute", "Gain", "EQ.on"} {
			value, err := readFloat(prefix + name)
			if err != nil {
				return err
			}
			row[name] = value
		}
		buses = append(buses, row)
	}
	encoder := json.NewEncoder(os.Stdout)
	encoder.SetIndent("", "  ")
	return encoder.Encode(map[string]any{"strips": strips, "buses": buses, "read_only": true})
}
