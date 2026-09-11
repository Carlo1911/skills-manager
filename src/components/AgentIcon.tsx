import { useState, type ReactNode } from "react";
import { Globe } from "lucide-react";
import { cn } from "../utils";
import { getAgentIconSrc, agentIconNeedsDarkInvert } from "../lib/agentIcons";

interface AgentIconProps {
  agentKey: string;
  /**
   * Icon key to render instead of `agentKey`. Used by custom agents, whose
   * generated key has no matching bundled icon but which may have picked
   * one from the shared set (see `iconKey` on `ToolInfo`).
   */
  iconOverride?: string | null;
  displayName?: string;
  className?: string;
  imageClassName?: string;
  fallback?: ReactNode;
}

export function AgentIcon({
  agentKey,
  iconOverride,
  displayName,
  className,
  imageClassName,
  fallback,
}: AgentIconProps) {
  const iconKey = iconOverride || agentKey;
  const src = getAgentIconSrc(iconKey);
  const [failedSrc, setFailedSrc] = useState<string | null>(null);
  const hasFailed = src === failedSrc;

  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center justify-center overflow-hidden rounded-md border border-border-subtle bg-surface",
        className
      )}
      title={displayName}
      aria-hidden="true"
    >
      {src && !hasFailed ? (
        <img
          src={src}
          alt=""
          draggable={false}
          className={cn(
            "h-full w-full object-contain",
            agentIconNeedsDarkInvert(iconKey) && "dark:invert",
            imageClassName
          )}
          onError={() => setFailedSrc(src)}
        />
      ) : (
        fallback ?? <Globe className="h-1/2 w-1/2 text-muted" />
      )}
    </span>
  );
}
