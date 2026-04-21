import { useState } from "react";
import { X } from "lucide-react";

export default function SettingsModal({ onClose }: { onClose: () => void }) {
  const [activeTab, setActiveTab] = useState("main-chat-model");

  return (
    <div className="fixed inset-0 bg-crust/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
      <div className="bg-base border border-surface1 rounded-xl shadow-2xl w-full max-w-2xl flex flex-col max-h-[80vh]">

        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-surface0">
          <h2 className="text-lg font-semibold text-text">Settings</h2>
          <button
            onClick={onClose}
            className="p-1 text-subtext0 hover:text-text hover:bg-surface0 rounded-md transition-colors"
          >
            <X size={20} />
          </button>
        </div>

        {/* Content */}
        <div className="flex flex-1 min-h-0">
          {/* Sidebar */}
          <div className="w-48 border-r border-surface0 p-2 flex flex-col gap-1 overflow-y-auto bg-mantle/30">
            {["General", "Main Chat Model", "Embeddings", "API Keys"].map((tab) => {
              const id = tab.toLowerCase().replace(/ /g, "-");
              return (
                <button
                  key={id}
                  onClick={() => setActiveTab(id)}
                  className={`text-left px-3 py-2 text-sm rounded-md transition-colors ${
                    activeTab === id
                      ? "bg-surface0 text-text font-medium"
                      : "text-subtext0 hover:text-text hover:bg-surface0/50"
                  }`}
                >
                  {tab}
                </button>
              );
            })}
          </div>

          {/* Settings Area */}
          <div className="flex-1 p-6 overflow-y-auto">
            {activeTab === "main-chat-model" && (
              <div className="space-y-6">
                <div>
                  <h3 className="text-sm font-medium text-text mb-3">Model Provider</h3>
                  <select className="w-full bg-mantle border border-surface1 rounded-md px-3 py-2 text-sm focus:outline-none focus:border-blue text-text">
                    <option>Anthropic</option>
                    <option>OpenAI / OpenRouter</option>
                    <option>Ollama (Local)</option>
                    <option>xAI</option>
                  </select>
                </div>

                <div>
                  <h3 className="text-sm font-medium text-text mb-3">Model Name</h3>
                  <input
                    type="text"
                    placeholder="e.g. claude-3-5-sonnet"
                    defaultValue="claude-3-5-sonnet"
                    className="w-full bg-mantle border border-surface1 rounded-md px-3 py-2 text-sm focus:outline-none focus:border-blue text-text"
                  />
                  <p className="text-xs text-subtext0 mt-2">
                    For Ollama, select the provider above and enter your model tag (e.g. <code>llama3.2</code>).
                  </p>
                </div>

                <div>
                  <h3 className="text-sm font-medium text-text mb-3">Base URL (Optional)</h3>
                  <input
                    type="text"
                    placeholder="http://localhost:11434/v1"
                    className="w-full bg-mantle border border-surface1 rounded-md px-3 py-2 text-sm focus:outline-none focus:border-blue text-text"
                  />
                </div>
              </div>
            )}

            {activeTab === "api-keys" && (
              <div className="space-y-6">
                <div>
                  <h3 className="text-sm font-medium text-text mb-3">Anthropic API Key</h3>
                  <input
                    type="password"
                    placeholder="sk-ant-..."
                    className="w-full bg-mantle border border-surface1 rounded-md px-3 py-2 text-sm focus:outline-none focus:border-blue text-text"
                  />
                </div>
                <div>
                  <h3 className="text-sm font-medium text-text mb-3">OpenAI / Compatible API Key</h3>
                  <input
                    type="password"
                    placeholder="sk-..."
                    className="w-full bg-mantle border border-surface1 rounded-md px-3 py-2 text-sm focus:outline-none focus:border-blue text-text"
                  />
                </div>
              </div>
            )}

            {activeTab === "general" && (
              <div className="text-sm text-subtext0">
                General application settings will appear here.
              </div>
            )}

            {activeTab === "embeddings" && (
              <div className="text-sm text-subtext0">
                Workspace embedding model settings will appear here.
              </div>
            )}
          </div>
        </div>

        {/* Footer */}
        <div className="p-4 border-t border-surface0 bg-mantle/50 flex justify-end gap-3">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm font-medium text-text hover:bg-surface0 rounded-md transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm font-medium bg-blue text-crust hover:bg-blue/90 rounded-md transition-colors"
          >
            Save Changes
          </button>
        </div>
      </div>
    </div>
  );
}
