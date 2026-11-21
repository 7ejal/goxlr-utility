// Audits the stock CPU-audio ownership chain; no USB or hardware access.
// Run: go run tools/capacity_routing_audit.go -firmware <901-or-909.bin>
package main

import (
	"crypto/sha256"
	"encoding/binary"
	"encoding/json"
	"flag"
	"fmt"
	"os"
)

type route struct {
	Index       int    `json:"index"`
	Source      string `json:"source"`
	Destination string `json:"destination"`
}

func main() {
	file := flag.String("firmware", "", "Exact preserved 901 or 909 image")
	flag.Parse()
	if err := audit(*file); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func audit(path string) error {
	b, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	hash := fmt.Sprintf("%x", sha256.Sum256(b))
	if hash != "5ff0d41144b47672c1486d7e7182ae260f7a36e6044860ddfd0f1afceb1436db" &&
		hash != "74555b3667dcb7150930ae544bdfcf732febf7bb6a0aa9b7cfd9b513ffa3c74b" {
		return fmt.Errorf("unrecognized firmware SHA256 %s", hash)
	}
	w := func(address uint32) uint32 { return binary.LittleEndian.Uint32(b[address-0x1ac:]) }
	literals := map[uint32]uint32{
		0x13d0: 0x2ee28, 0x1408: 0x3666c, 0x140c: 0x3ca5c,
		0x1428: 0x3ca5c, 0x142c: 0x101c, 0x1430: 0x1024,
		0x2ee3c: 0xf2c, 0x3666c: 4,
		0xefcc: 0x3ed60, 0xefd0: 0x3ed8c,
		0xf074: 0x3ed64, 0xf078: 0x3ed88, 0xf07c: 0x3ed84, 0xf080: 0x3ed8c,
		0xf130: 0x3ed84, 0xf134: 0x3ed60, 0xf138: 0x3ed8c,
		0xf13c: 0x3ed88, 0xf140: 0x3ed64,
		0x101c: 0xe3a00001, 0x1020: 0xe12fff1e,
	}
	for address, expected := range literals {
		if w(address) != expected {
			return fmt.Errorf("anchor %x: got%x expected%x", address, w(address), expected)
		}
	}
	for address, target := range map[uint32]uint32{0xbb3c: 0x13e4, 0xbccc: 0x1410, 0x13f8: 0x12c4, 0x1424: 0xf0e8} {
		instruction := w(address)
		if instruction&0x0e000000 != 0x0a000000 {
			return fmt.Errorf("not A32 branch at%x", address)
		}
		displacement := int32(instruction<<8) >> 6
		actual := uint32(int32(address+8) + displacement)
		if actual != target {
			return fmt.Errorf("branch%x targets%x, expected%x", address, actual, target)
		}
	}
	var routes []route
	names := map[uint32]string{1: "ins", 6: "mix", 7: "cpu", 12: "usb"}
	for i := 0; i < 55; i++ {
		v := w(0x36590 + uint32(4*i))
		dst, src := (v>>7)&31, (v>>19)&127
		if dst == 7 || src == 7 || (dst == 1 && ((v&127) == 2 || (v&127) == 3)) {
			routes = append(routes, route{i, fmt.Sprintf("%s%d", names[src], (v>>12)&127), fmt.Sprintf("%s%d", names[dst], v&127)})
		}
	}
	if len(routes) != 10 {
		return fmt.Errorf("expected 8 CPU routes and 2 headphone destinations, got%d", len(routes))
	}
	references := map[string][]string{}
	for _, pointer := range []uint32{0x3ed64, 0x3ed88, 0x3ed60, 0x3ed84, 0x3ed8c} {
		key := fmt.Sprintf("ARM:%X", pointer)
		for address := uint32(0x200); address < 0x3a458; address += 4 {
			if w(address) == pointer {
				references[key] = append(references[key], fmt.Sprintf("ARM:%X", address))
			}
		}
	}
	report := map[string]any{
		"firmware_sha256": hash,
		"status":          "VERIFIED ORIGINAL CODE AND OFFLINE EXECUTION; NOT LIVE TELEMETRY",
		"registration":    map[string]any{"constructor_call": "BB3C ->13E4 ->12C4", "object_pointer": "ARM:3CA5C", "configured_frames": 1 << w(0x3666c), "start_call": "BCCC ->1410 ->F0E8", "hardware_config": "C4000400=C0000003", "interrupt_handler": "EFD4", "top_callback": "101C returns1", "deferred_callback": "EFA4 ->1024 ->vtable+14 ->F2C", "callback_slot": "ARM:3ED60", "top_slot": "ARM:3ED84", "context_slot": "ARM:3ED8C"},
		"owned_counters": []map[string]string{
			{"address": "ARM:3ED64", "width": "u32 wrapping", "increment": "CPU-audio status bit22", "meaning": "event count only; overrun identity unproved"},
			{"address": "ARM:3ED88", "width": "u32 wrapping", "increment": "CPU-audio status bit23", "meaning": "event count only; not cycles or utilization"},
		},
		"routes":                  routes,
		"direct_pointer_literals": references,
		"capacity_limits":         []string{"909 exposes neither these ARM counters nor arbitrary RAM reads", "CPU-audio events are not per-core external DSP utilization", "16-frame ARM CPU callback is not proof of external DSP block size", "existing CPU paths carry USB inputs before the DICE mixer; not the headphone postmix", "no physical chip model, oscillator, PLL clock, free cycles, heap/stack high-water marks or DAC assignment measured"},
	}
	encoder := json.NewEncoder(os.Stdout)
	encoder.SetIndent("", "  ")
	return encoder.Encode(report)
}
