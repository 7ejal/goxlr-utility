// Inspect the known 901/908/909 images offline; never opens USB or changes firmware.
// Run: go run tools/goxlr_dynamics_map.go -firmware <image.bin>
package main

import (
	"crypto/sha256"
	"encoding/binary"
	"encoding/json"
	"flag"
	"fmt"
	"math"
	"os"
)

type control struct {
	Key, Parameter, Mailbox, Callback, Command, Object, Conversion uint32
	Minimum, Maximum, Default                                      int32
	Descriptor                                                     []uint32
	Dispatch                                                       [2]uint32
}

type eqControl struct {
	Band  int
	Field string
	control
}

func main() {
	firmware := flag.String("firmware", "", "known 901/908/909 image; read only")
	eqDescriptors := flag.Bool("eq-descriptors", false, "decode only the 36 EQ descriptors; defaults are not effective boot/reset settings")
	flag.Parse()
	if err := inspect(*firmware, *eqDescriptors); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func inspect(path string, eqDescriptors bool) error {
	b, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	hash := fmt.Sprintf("%x", sha256.Sum256(b))
	if hash != "5ff0d41144b47672c1486d7e7182ae260f7a36e6044860ddfd0f1afceb1436db" &&
		hash != "c8607989063552e5a412ad3b29b4e1db355d9b737dcc59d972a5349fbfaa970f" &&
		hash != "74555b3667dcb7150930ae544bdfcf732febf7bb6a0aa9b7cfd9b513ffa3c74b" {
		return fmt.Errorf("unrecognized image SHA256: %s", hash)
	}
	word := func(offset int) uint32 { return uint32(b[offset])<<16 | uint32(b[offset+1])<<8 | uint32(b[offset+2]) }
	if len(b) != 640044 || string(b[0x3a2b0:0x3a2ba]) != "TCHDSP_V2:" {
		return fmt.Errorf("unexpected container")
	}
	p := make(map[uint32]uint32)
	offset := 0x3a2b0 + 22
	offset += 6 + 3*int(word(offset))
	for {
		n, address, flags := word(offset), word(offset+3), word(offset+6)
		offset += 9
		if n == 0 {
			break
		}
		if flags != 0 || uint64(offset)+3*uint64(n) > uint64(len(b)) {
			return fmt.Errorf("invalid DSP record")
		}
		for i := uint32(0); i < n; i++ {
			p[address+i] = word(offset + 3*int(i))
		}
		offset += 3 * int(n)
	}
	// These original bootstrap descriptors own the two ROM-backed X tables.
	x := make(map[uint32]uint32)
	for _, d := range []struct{ index, destination, source, count uint32 }{
		{91, 0x427100, 0x40bf08, 0x831}, {96, 0x41a015, 0x40c739, 0x2218},
	} {
		a := uint32(0x40ab6d) + 4*d.index
		if p[a] != 0x201 || p[a+1] != d.destination || p[a+2] != d.source || p[a+3] != d.count {
			return fmt.Errorf("boot descriptor changed")
		}
		for i := uint32(0); i < d.count; i++ {
			v, ok := p[d.source+i]
			if !ok {
				return fmt.Errorf("missing source word")
			}
			x[d.destination+i] = v
		}
	}
	// Check the native dispatcher calls, object registration and tables against serialized opcodes.
	anchors := map[uint32][]uint32{
		0x400405: {0x0a70d2, 0x00001e}, 0x400426: {0x0a70d2, 0x00000d},
		0x400429: {0x0a70d2, 0x00001c}, 0x40042d: {0x0a70d3, 0x000021},
		0x402a33: {0x0bf080, 0x0344d1}, 0x402a4e: {0x0bf080, 0x0334a3},
		0x0344de: {0x0344e3, 0x0344ed, 0x0344ff, 0x034509, 0x034519},
		0x0334af: {0x0334b5, 0x0334b9, 0x0334cc, 0x0334d6, 0x0334e6, 0x0334ef},
		0x0344e4: {0x62f400, 0x41bd70}, 0x034500: {0x60f400, 0x41bd8e},
		0x03450a: {0x66f400, 0x41bdb6}, 0x0334cd: {0x63f400, 0x41b970},
		0x0334d7: {0x62f400, 0x41b998}, 0x0334e7: {0x63f400, 0x41b9c0},
	}
	for a, words := range anchors {
		for i, w := range words {
			if p[a+uint32(i)] != w {
				return fmt.Errorf("opcode mismatch P:%06X", a+uint32(i))
			}
		}
	}
	signed := func(v uint32) int32 { return int32(v<<8) >> 8 }
	if eqDescriptors {
		starts := [...]uint32{0x126, 0xf8, 0x113, 0x129, 0x116, 0x11d, 0x12c, 0x120, 0x109, 0x12f, 0xf5, 0x10c}
		var records []eqControl
		for band, start := range starts {
			for field, name := range []string{"Frequency", "Gain", "Width"} {
				key := start + uint32(field)
				index, ok := x[0x41abe9+key]
				if !ok || index == 0xffffff {
					return fmt.Errorf("missing EQ key %03X", key)
				}
				address := uint32(0x41a391) + 8*index
				row := make([]uint32, 8)
				for i := range row {
					value, ok := x[address+uint32(i)]
					if !ok {
						return fmt.Errorf("missing EQ descriptor word")
					}
					row[i] = value
				}
				if row[4] != key {
					return fmt.Errorf("EQ descriptor key mismatch")
				}
				route := x[0x427441+(row[6]&0xffff)]
				if route>>22 != 0 {
					return fmt.Errorf("unrecognized EQ dispatch class")
				}
				slot := (route >> 10) & 0xfff
				pair := [2]uint32{x[0x427536+2*slot], x[0x427537+2*slot]}
				callback := x[0x427526+(pair[1]&255)]
				records = append(records, eqControl{band + 1, name, control{key, index, slot, callback, pair[0] & 0xffff,
					(pair[1] >> 8) & 255, route & 1023, signed(row[0]), signed(row[2]), signed(row[3]), row, pair}})
			}
		}
		enc := json.NewEncoder(os.Stdout)
		enc.SetIndent("", "  ")
		return enc.Encode(map[string]any{"image_sha256": hash, "eq_descriptors": records,
			"default_scope": "Serialized ROM descriptor field 3 only; effective boot/runtime defaults are not established and these values do not authorize reset."})
	}
	var controls []control
	for key := uint32(1); key <= 0x167; key++ {
		index := x[0x41abe9+key]
		if index == 0xffffff {
			continue
		}
		a := uint32(0x41a391) + 8*index
		if x[a+7]&0x2000 == 0 {
			continue
		}
		route := x[0x427441+(x[a+6]&0xffff)]
		if route>>22 != 0 {
			continue
		}
		slot := (route >> 10) & 0xfff
		pair := [2]uint32{x[0x427536+2*slot], x[0x427537+2*slot]}
		callback := x[0x427526+(pair[1]&255)]
		if callback != 0x402a32 && callback != 0x402a4c {
			continue
		}
		row := make([]uint32, 8)
		for i := range row {
			row[i] = x[a+uint32(i)]
		}
		if row[4] != key {
			return fmt.Errorf("parameter key mismatch")
		}
		controls = append(controls, control{key, index, slot, callback, pair[0] & 0xffff, (pair[1] >> 8) & 255, route & 1023, signed(row[0]), signed(row[2]), signed(row[3]), row, pair})
	}
	if len(controls) != 21 {
		return fmt.Errorf("expected 21 dynamics mappings, got %d", len(controls))
	}
	// Logical board routes only. Physical ins wiring is still unproved.
	var routes []map[string]uint32
	for i := 0; i < 55; i++ {
		v := binary.LittleEndian.Uint32(b[0x36590-0x1ac+4*i:])
		if v>>19&127 == 6 {
			routes = append(routes, map[string]uint32{"index": uint32(i), "mix_channel": v >> 12 & 127, "destination_block": v >> 7 & 31, "destination_channel": v & 127})
		}
	}
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	count, address := x[0x42740b+12], x[0x42740b+13]
	if count == 0 || count > 256 {
		return fmt.Errorf("invalid de-esser scalar conversion table")
	}
	var conversion [][2]int32
	for i := uint32(0); i < count; i++ {
		conversion = append(conversion, [2]int32{signed(x[address+i]), signed(x[address+count+i])})
	}
	tables := map[string][]float64{}
	for _, table := range []struct {
		name           string
		address, count uint32
	}{
		{"deesser_attack_seconds", 0x41b970, 20}, {"deesser_release_seconds", 0x41b998, 20},
		{"compressor_ratios", 0x41bd70, 15}, {"compressor_attack_seconds", 0x41bd8e, 20},
		{"compressor_release_seconds", 0x41bdb6, 20},
	} {
		for i := uint32(0); i < table.count; i++ {
			exponent, mantissa := x[table.address+2*i], x[table.address+2*i+1]
			tables[table.name] = append(tables[table.name], math.Ldexp(float64(signed(mantissa))/8388608, int(signed(exponent))-127))
		}
	}
	return enc.Encode(map[string]any{"image_sha256": hash, "controls": controls, "logical_mix_routes": routes,
		"global_slot_to_root_child":       map[string]int{"5": 30, "16": 13, "17": 28, "18": 33},
		"decoded_native_tables":           tables,
		"deesser_mode_words":              []uint32{x[0x41b9c0], x[0x41b9c1], x[0x41b9c2]},
		"deesser_scalar_conversion_pairs": conversion,
		"limits":                          []string{"Static serialized-image evidence; no live timing, memory or physical PCM ownership measurement.", "ROM ranges are not permission to expand UI ranges. Existing 908 bridge accepts mic EQ keys only.", "Object indices select X:419E2C pointer slots, not physical inputs/outputs. No new feature is enabled."}})
}
