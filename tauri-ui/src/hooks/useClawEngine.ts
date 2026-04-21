import { useState } from "react";
import { Command } from "@tauri-apps/plugin-shell";

export interface Message {
  role: "user" | "assistant" | "system";
  content: string;
  thoughtProcess?: string;
  thinking?: string; // Adding for backwards compatibility with UI
}

export function useClawEngine() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [isProcessing, setIsProcessing] = useState(false);
  const [tokenUsage, setTokenUsage] = useState(0);

  const sendMessage = async (content: string) => {
    setIsProcessing(true);
    const newMessage: Message = { role: "user", content };
    setMessages((prev) => [...prev, newMessage]);

    try {
      // In a real scenario, you would probably maintain a persistent daemon.
      // Here we simulate spawning the claw binary for a single completion
      // using the sidecar configuration.
      const command = Command.sidecar("claw", ["chat", content]);

      const output = await command.execute();

      if (output.code === 0) {
        // Parse the JSON output from claw if applicable
        let responseContent = output.stdout;
        let thoughtProcess = undefined;

        try {
          // If claw returns JSON with thought process
          const parsed = JSON.parse(output.stdout);
          if (parsed.content) {
            responseContent = parsed.content;
          }
          if (parsed.thoughtProcess) {
            thoughtProcess = parsed.thoughtProcess;
          }
          if (parsed.tokenUsage) {
            setTokenUsage((prev) => prev + parsed.tokenUsage);
          }
        } catch (e) {
          // Fallback to raw stdout
        }

        const assistantMessage: Message = {
          role: "assistant",
          content: responseContent,
          thoughtProcess,
          thinking: thoughtProcess,
        };
        setMessages((prev) => [...prev, assistantMessage]);
      } else {
        const errorMessage: Message = {
          role: "assistant",
          content: `Error: ${output.stderr}`,
        };
        setMessages((prev) => [...prev, errorMessage]);
      }
    } catch (error) {
      console.error("Claw engine error:", error);
      const errorMessage: Message = {
        role: "assistant",
        content: `Error connecting to AI engine: ${error}`,
      };
      setMessages((prev) => [...prev, errorMessage]);
    } finally {
      setIsProcessing(false);
    }
  };

  return {
    messages,
    sendMessage,
    isProcessing,
    tokenUsage
  };
}
