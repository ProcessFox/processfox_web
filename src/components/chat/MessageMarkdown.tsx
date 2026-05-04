import { memo, type AnchorHTMLAttributes, type MouseEvent } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import { openUrl } from "@tauri-apps/plugin-opener";

import { cn } from "@/lib/utils";

type Props = {
  text: string;
  className?: string;
};

/** Renders an assistant chat bubble's text as Markdown.
 *
 *  - GFM tables, autolinks, strikethrough, task lists.
 *  - Single newlines render as line breaks (matches LLM intent).
 *  - Raw HTML is disabled (react-markdown default) so assistant output
 *    cannot inject script tags or arbitrary markup.
 *  - Links open in the user's default browser via the Tauri opener plugin
 *    instead of navigating the WebView. */
function MessageMarkdownInner({ text, className }: Props) {
  return (
    <div
      className={cn(
        "break-words text-sm leading-[19px] text-foreground",
        className,
      )}
    >
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkBreaks]}
        components={MARKDOWN_COMPONENTS}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}

export const MessageMarkdown = memo(MessageMarkdownInner);

const MARKDOWN_COMPONENTS: Components = {
  a: ({ href, children, ...rest }: AnchorHTMLAttributes<HTMLAnchorElement>) => {
    const onClick = (e: MouseEvent<HTMLAnchorElement>) => {
      e.preventDefault();
      if (!href) return;
      openUrl(href).catch((err) => console.warn("openUrl failed", err));
    };
    return (
      <a
        {...rest}
        href={href}
        onClick={onClick}
        className="text-primary underline underline-offset-2 hover:opacity-80"
      >
        {children}
      </a>
    );
  },
  p: ({ children }) => <p className="mb-2 last:mb-0">{children}</p>,
  ul: ({ children }) => (
    <ul className="mb-2 list-disc pl-5 last:mb-0 [&_ul]:mb-0 [&_ol]:mb-0">
      {children}
    </ul>
  ),
  ol: ({ children }) => (
    <ol className="mb-2 list-decimal pl-5 last:mb-0 [&_ul]:mb-0 [&_ol]:mb-0">
      {children}
    </ol>
  ),
  li: ({ children }) => <li className="mb-0.5 last:mb-0">{children}</li>,
  h1: ({ children }) => (
    <h1 className="mb-2 mt-1 text-base font-semibold first:mt-0">{children}</h1>
  ),
  h2: ({ children }) => (
    <h2 className="mb-2 mt-1 text-base font-semibold first:mt-0">{children}</h2>
  ),
  h3: ({ children }) => (
    <h3 className="mb-1.5 mt-1 text-sm font-semibold first:mt-0">{children}</h3>
  ),
  h4: ({ children }) => (
    <h4 className="mb-1.5 mt-1 text-sm font-semibold first:mt-0">{children}</h4>
  ),
  h5: ({ children }) => (
    <h5 className="mb-1 mt-1 text-sm font-semibold first:mt-0">{children}</h5>
  ),
  h6: ({ children }) => (
    <h6 className="mb-1 mt-1 text-sm font-semibold first:mt-0">{children}</h6>
  ),
  blockquote: ({ children }) => (
    <blockquote className="mb-2 border-l-2 border-border pl-3 italic text-muted-foreground last:mb-0">
      {children}
    </blockquote>
  ),
  hr: () => <hr className="my-3 border-border" />,
  code: ({ className, children, ...rest }) => {
    const isBlock = /language-/.test(className ?? "");
    if (isBlock) {
      return (
        <code className={cn("font-mono text-[12px]", className)} {...rest}>
          {children}
        </code>
      );
    }
    return (
      <code
        className="rounded-sm bg-background/60 px-1 py-0.5 font-mono text-[12px]"
        {...rest}
      >
        {children}
      </code>
    );
  },
  pre: ({ children }) => (
    <pre className="mb-2 overflow-x-auto rounded-sm bg-background/60 px-2 py-1.5 font-mono text-[12px] last:mb-0">
      {children}
    </pre>
  ),
  table: ({ children }) => (
    <div className="mb-2 overflow-x-auto last:mb-0">
      <table className="w-full border-collapse text-left text-[12px]">
        {children}
      </table>
    </div>
  ),
  thead: ({ children }) => (
    <thead className="border-b border-border">{children}</thead>
  ),
  tbody: ({ children }) => <tbody>{children}</tbody>,
  tr: ({ children }) => <tr className="border-b border-border/60 last:border-b-0">{children}</tr>,
  th: ({ children }) => (
    <th className="px-2 py-1 font-semibold align-top">{children}</th>
  ),
  td: ({ children }) => <td className="px-2 py-1 align-top">{children}</td>,
};
