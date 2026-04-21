import { Activity, Thermometer, Database } from "lucide-react";
import { useHardwareMonitor } from "../../hooks/useHardwareMonitor";

export default function RightPanel() {
  const { metrics } = useHardwareMonitor();
  const vramPercent = metrics.vramTotal > 0 ? (metrics.vramUsed / metrics.vramTotal) * 100 : 0;

  return (
    <div className="w-64 bg-mantle border-l border-surface0 flex flex-col h-full flex-shrink-0">
      <div className="p-4 border-b border-surface0 flex items-center gap-2">
        <Activity size={18} className="text-green" />
        <h2 className="font-semibold text-sm">System Monitor</h2>
      </div>

      <div className="p-4 space-y-6 flex-1 overflow-y-auto">
        {/* GPU Temperature */}
        <div className="space-y-2">
          <div className="flex items-center justify-between text-sm">
            <div className="flex items-center gap-2 text-subtext1">
              <Thermometer size={16} />
              <span>GPU Temp</span>
            </div>
            <span className={`font-medium ${metrics.temp > 80 ? 'text-red' : metrics.temp > 65 ? 'text-peach' : 'text-text'}`}>
              {metrics.temp}°C
            </span>
          </div>
          <div className="h-2 w-full bg-surface0 rounded-full overflow-hidden">
            <div
              className={`h-full transition-all duration-500 ${metrics.temp > 80 ? 'bg-red' : metrics.temp > 65 ? 'bg-peach' : 'bg-green'}`}
              style={{ width: `${Math.min(100, (metrics.temp / 100) * 100)}%` }}
            />
          </div>
        </div>

        {/* VRAM Usage */}
        <div className="space-y-2">
          <div className="flex items-center justify-between text-sm">
            <div className="flex items-center gap-2 text-subtext1">
              <Database size={16} />
              <span>VRAM</span>
            </div>
            <span className="font-medium text-text">
              {metrics.vramUsed} / {metrics.vramTotal} MB
            </span>
          </div>
          <div className="h-2 w-full bg-surface0 rounded-full overflow-hidden">
            <div
              className={`h-full bg-blue transition-all duration-500 ${vramPercent > 90 ? 'bg-red' : ''}`}
              style={{ width: `${vramPercent}%` }}
            />
          </div>
        </div>

        {/* GPU Utilization */}
        <div className="space-y-2">
          <div className="flex items-center justify-between text-sm">
            <div className="flex items-center gap-2 text-subtext1">
              <Activity size={16} />
              <span>GPU Load</span>
            </div>
            <span className="font-medium text-text">
              {metrics.utilization}%
            </span>
          </div>
          <div className="h-2 w-full bg-surface0 rounded-full overflow-hidden">
            <div
              className="h-full bg-mauve transition-all duration-500"
              style={{ width: `${metrics.utilization}%` }}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
