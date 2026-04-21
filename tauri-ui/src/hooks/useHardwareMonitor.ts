import { useState, useEffect } from "react";
import { Command } from "@tauri-apps/plugin-shell";

export interface HardwareMetrics {
  gpu_temp: number;
  vram_usage: number;
  temp: number; // Aliases for UI compatibility
  vramUsed: number;
  vramTotal: number;
  utilization: number;
}

export function useHardwareMonitor() {
  const [metrics, setMetrics] = useState<HardwareMetrics>({ gpu_temp: 0, vram_usage: 0, temp: 0, vramUsed: 0, vramTotal: 100, utilization: 0 });
  const [isConnected, setIsConnected] = useState(false);

  useEffect(() => {
    let interval: ReturnType<typeof setInterval>;

    const fetchMetrics = async () => {
      try {
        // Execute the python hardware daemon as a sidecar
        const command = Command.sidecar("hardware_daemon");
        const output = await command.execute();

        if (output.code === 0) {
          const parsed = JSON.parse(output.stdout);
          setMetrics({
            gpu_temp: parsed.gpu_temp,
            vram_usage: parsed.vram_usage,
            temp: parsed.gpu_temp,
            vramUsed: parsed.vram_usage,
            vramTotal: 100, // Or whatever max scale
            utilization: parsed.vram_usage // Approximated
          });
          setIsConnected(true);
        } else {
          console.error("Hardware daemon error:", output.stderr);
          setIsConnected(false);
        }
      } catch (err) {
        console.error("Failed to connect to hardware daemon:", err);
        setIsConnected(false);
      }
    };

    fetchMetrics();
    interval = setInterval(fetchMetrics, 3000); // Poll every 3 seconds

    return () => clearInterval(interval);
  }, []);

  return { metrics, isConnected };
}
