// hello-go's one service component.
//
// Genuinely compiled by a real `go build` into a real, standalone native
// executable before this ever runs (see the go-backend adapter and
// docs/decisions/0015-go-backend-adapter.md) -- not interpreted. It is
// invoked (after compilation) as:
//
//	service <run-config.json>
//
// where <run-config.json> holds this component's merged inputs.
package main

import (
	"encoding/json"
	"fmt"
	"os"
)

type greeterConfig struct {
	Greeting string `json:"greeting"`
	Count    int    `json:"count"`
}

func loadConfig() greeterConfig {
	config := greeterConfig{Greeting: "Hello from the OmniForge go-backend adapter.", Count: 1}
	if len(os.Args) < 2 {
		return config
	}
	data, err := os.ReadFile(os.Args[1])
	if err != nil {
		return config
	}
	var parsed struct {
		Greeting *string `json:"greeting"`
		Count    *int    `json:"count"`
	}
	if err := json.Unmarshal(data, &parsed); err != nil {
		return config
	}
	if parsed.Greeting != nil {
		config.Greeting = *parsed.Greeting
	}
	if parsed.Count != nil {
		config.Count = *parsed.Count
	}
	return config
}

func main() {
	config := loadConfig()
	for i := 1; i <= config.Count; i++ {
		fmt.Printf("[%d/%d] %s\n", i, config.Count, config.Greeting)
	}
}
