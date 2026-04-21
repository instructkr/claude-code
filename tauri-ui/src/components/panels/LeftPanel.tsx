import { useState } from "react";
import { FolderGit2, MessageSquare, FileCode2 } from "lucide-react";

export default function LeftPanel() {
  const [activeTab, setActiveTab] = useState<"files" | "history">("files");

  return (
    <div className="w-64 bg-mantle border-r border-surface0 flex flex-col h-full flex-shrink-0">
      <div className="flex border-b border-surface0">
        <button
          className={`flex-1 p-3 text-sm font-medium flex items-center justify-center gap-2 transition-colors ${
            activeTab === "files" ? "text-blue border-b-2 border-blue bg-surface0/30" : "text-subtext0 hover:text-text hover:bg-surface0/20"
          }`}
          onClick={() => setActiveTab("files")}
        >
          <FolderGit2 size={16} />
          Workspace
        </button>
        <button
          className={`flex-1 p-3 text-sm font-medium flex items-center justify-center gap-2 transition-colors ${
            activeTab === "history" ? "text-blue border-b-2 border-blue bg-surface0/30" : "text-subtext0 hover:text-text hover:bg-surface0/20"
          }`}
          onClick={() => setActiveTab("history")}
        >
          <MessageSquare size={16} />
          History
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-2">
        {activeTab === "files" ? (
          <div className="space-y-1">
            <div className="flex items-center gap-2 p-1.5 rounded hover:bg-surface0 cursor-pointer text-sm text-subtext1">
              <FileCode2 size={14} className="text-mauve" />
              <span>src/main.rs</span>
            </div>
            <div className="flex items-center gap-2 p-1.5 rounded hover:bg-surface0 cursor-pointer text-sm text-subtext1">
              <FolderGit2 size={14} className="text-blue" />
              <span>components/</span>
            </div>
          </div>
        ) : (
          <div className="space-y-2 p-1">
            <div className="p-2 rounded bg-surface0/50 hover:bg-surface0 cursor-pointer">
              <div className="text-sm font-medium text-text truncate">Implement UI layout</div>
              <div className="text-xs text-subtext0 mt-1">2 hours ago</div>
            </div>
            <div className="p-2 rounded bg-surface0/50 hover:bg-surface0 cursor-pointer">
              <div className="text-sm font-medium text-text truncate">Fix bug in parsing</div>
              <div className="text-xs text-subtext0 mt-1">Yesterday</div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
