// Inspect the known 901/908 images offline; never opens USB or changes firmware.
// Run: go run tools/goxlr_dynamics_map.go -firmware <image.bin>
package main

import (
	"crypto/sha256"
	"encoding/binary"
	"encoding/json"
	"flag"
	"fmt"
	"os"
)

type control struct {
	Key, Parameter, Mailbox, Callback, Command, Object, Conversion uint32
	Minimum, Maximum, Default                                      int32
	Descriptor                                                     []uint32
	Dispatch                                                       [2]uint32
}

func main() {
	firmware := flag.String("firmware", "", "known 901 or tested 908 image; read only")
	flag.Parse()
	if err := inspect(*firmware); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func inspect(path string) error {
	b, err := os.ReadFile(path)
	if err != nil {
		return err
	}
	hash := fmt.Sprintf("%x", sha256.Sum256(b))
	if hash != "5ff0d41144b47672c1486d7e7182ae260f7a36e6044860ddfd0f1afceb1436db" &&
		hash != "c8607989063552e5a412ad3b29b4e1db355d9b737dcc59d972a5349fbfaa970f" {
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
	// Check the native dispatcher calls and tables directly against serialized opcodes.
	anchors := map[uint32][]uint32{
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
	return enc.Encode(map[string]any{"image_sha256": hash, "controls": controls, "logical_mix_routes": routes,
		"deesser_mode_words":              []uint32{x[0x41b9c0], x[0x41b9c1], x[0x41b9c2]},
		"deesser_scalar_conversion_pairs": conversion,
		"limits":                          []string{"Static serialized-image evidence; no live timing, memory or physical PCM ownership measurement.", "ROM ranges are not permission to expand UI ranges. Existing 908 bridge accepts mic EQ keys only.", "Object indices select X:419E2C pointer slots, not physical inputs/outputs. No new feature is enabled."}})
}
