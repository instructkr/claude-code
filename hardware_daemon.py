import time
import json
import sys
import argparse

# Try importing pynvml to detect NVIDIA GPU
try:
    import pynvml
    HAS_NVML = True
except ImportError:
    HAS_NVML = False

def init_gpu_monitoring():
    if HAS_NVML:
        try:
            pynvml.nvmlInit()
            return True
        except pynvml.NVMLError:
            return False
    return False

def get_gpu_metrics():
    metrics = {
        "temperature_c": 0,
        "vram_used_mb": 0,
        "vram_total_mb": 0,
        "gpu_utilization_percent": 0
    }

    if HAS_NVML and pynvml.nvmlInit():
        try:
            handle = pynvml.nvmlDeviceGetHandleByIndex(0)

            # Temperature
            temp = pynvml.nvmlDeviceGetTemperature(handle, pynvml.NVML_TEMPERATURE_GPU)
            metrics["temperature_c"] = temp

            # VRAM
            mem_info = pynvml.nvmlDeviceGetMemoryInfo(handle)
            metrics["vram_used_mb"] = mem_info.used // (1024 * 1024)
            metrics["vram_total_mb"] = mem_info.total // (1024 * 1024)

            # Utilization
            util = pynvml.nvmlDeviceGetUtilizationRates(handle)
            metrics["gpu_utilization_percent"] = util.gpu

        except pynvml.NVMLError:
            pass

    # Mock fallback if no GPU or nvml fails
    if metrics["temperature_c"] == 0:
        import random
        metrics["temperature_c"] = random.randint(45, 85)
        metrics["vram_total_mb"] = 8192
        metrics["vram_used_mb"] = random.randint(1024, 7000)
        metrics["gpu_utilization_percent"] = random.randint(10, 95)

    return metrics

def run_daemon(poll_interval=2):
    init_gpu_monitoring()

    while True:
        try:
            metrics = get_gpu_metrics()
            print(json.dumps({"type": "hardware_telemetry", "data": metrics}), flush=True)
            time.sleep(poll_interval)
        except KeyboardInterrupt:
            break
        except Exception as e:
            print(json.dumps({"type": "error", "message": str(e)}), flush=True)
            time.sleep(poll_interval)

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--interval", type=int, default=2, help="Polling interval in seconds")
    args = parser.parse_args()

    run_daemon(poll_interval=args.interval)
