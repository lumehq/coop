import { commands } from "@/commands";
import { cn, getReceivers, groupEventByDate, time, upload } from "@/commons";
import { Spinner } from "@/components/spinner";
import { User } from "@/components/user";
import { CoopIcon } from "@/icons/coop";
import { ArrowUp, Paperclip, X } from "@phosphor-icons/react";
import * as ScrollArea from "@radix-ui/react-scroll-area";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createLazyFileRoute } from "@tanstack/react-router";
import { listen } from "@tauri-apps/api/event";
import { message } from "@tauri-apps/plugin-dialog";
import type { NostrEvent } from "nostr-tools";
import {
  type Dispatch,
  type RefObject,
  type SetStateAction,
  useCallback,
  useRef,
  useState,
  useTransition,
} from "react";
import { useEffect } from "react";
import { Virtualizer, type VirtualizerHandle } from "virtua";

type EventPayload = {
  event: string;
  sender: string;
};

export const Route = createLazyFileRoute("/$account/_layout/chats/$id")({
  component: Screen,
});

function Screen() {
  return (
    <div className="size-full flex flex-col">
      <Header />
      <List />
      <Form />
    </div>
  );
}

function Header() {
  const { account, id } = Route.useParams();
  const { platform } = Route.useRouteContext();

  return (
    <div
      data-tauri-drag-region
      className={cn(
        "h-12 shrink-0 flex items-center justify-between border-b border-neutral-100 dark:border-neutral-800",
        platform === "windows" ? "pl-3.5 pr-[150px]" : "px-3.5",
      )}
    >
      <div className="z-[200]">
        <div className="flex -space-x-1 overflow-hidden">
          <User.Provider pubkey={account}>
            <User.Root className="size-8 rounded-full inline-block ring-2 ring-white dark:ring-neutral-900">
              <User.Avatar className="size-8 rounded-full" />
            </User.Root>
          </User.Provider>
          <User.Provider pubkey={id}>
            <User.Root className="size-8 rounded-full inline-block ring-2 ring-white dark:ring-neutral-900">
              <User.Avatar className="size-8 rounded-full" />
            </User.Root>
          </User.Provider>
        </div>
      </div>
      <div className="flex items-center gap-2">
        <div className="h-7 inline-flex items-center justify-center gap-1.5 px-2 rounded-full bg-neutral-100 dark:bg-neutral-900">
          <span className="relative flex size-2">
            <span className="animate-ping absolute inline-flex size-full rounded-full bg-teal-400 opacity-75" />
            <span className="relative inline-flex rounded-full size-2 bg-teal-500" />
          </span>
          <div className="text-xs leading-tight">Connected</div>
        </div>
      </div>
    </div>
  );
}

function List() {
  const { account, id } = Route.useParams();
  const { isLoading, isError, data } = useQuery({
    queryKey: ["chats", id],
    queryFn: async () => {
      const res = await commands.getChatMessages(id);

      if (res.status === "ok") {
        const raw = res.data;
        const events: NostrEvent[] = raw.map((item) => JSON.parse(item));

        return events;
      } else {
        throw new Error(res.error);
      }
    },
    select: (data) => {
      const groups = groupEventByDate(data);
      return Object.entries(groups).reverse();
    },
    refetchOnWindowFocus: false,
  });

  const queryClient = useQueryClient();
  const scrollRef = useRef<HTMLDivElement>(null);
  const ref = useRef<VirtualizerHandle>(null);
  const shouldStickToBottom = useRef(true);

  const renderItem = useCallback(
    (item: NostrEvent, idx: number) => {
      const self = account === item.pubkey;

      return (
        <div
          key={idx + item.id}
          className="flex items-center justify-between gap-3 my-1.5 px-3 border-l-2 border-transparent hover:border-blue-400"
        >
          <div
            className={cn(
              "flex-1 min-w-0 inline-flex",
              self ? "justify-end" : "justify-start",
            )}
          >
            <div
              className={cn(
                "select-text py-2 px-3 w-fit max-w-[400px] text-pretty break-message",
                !self
                  ? "bg-neutral-100 dark:bg-neutral-800 rounded-tl-3xl rounded-tr-3xl rounded-br-3xl rounded-bl-md"
                  : "bg-blue-500 text-white rounded-tl-3xl rounded-tr-3xl rounded-br-md rounded-bl-3xl",
              )}
            >
              <Message text={item.content} />
            </div>
          </div>
          <div className="shrink-0 w-16 flex items-center justify-end">
            <span className="text-xs text-right text-neutral-600 dark:text-neutral-400">
              {time(item.created_at)}
            </span>
          </div>
        </div>
      );
    },
    [data],
  );

  useEffect(() => {
    const unlisten = listen<EventPayload>("event", async (data) => {
      const event: NostrEvent = JSON.parse(data.payload.event);
      const sender = data.payload.sender;
      const receivers = getReceivers(event.tags);
      const group = [account, id];

      if (!group.includes(sender)) return;
      if (!group.some((item) => receivers.includes(item))) return;

      queryClient.setQueryData(["chats", id], (prevEvents: NostrEvent[]) => {
        if (!prevEvents) return [event];
        return [event, ...prevEvents];
      });

      await queryClient.invalidateQueries({ queryKey: ["chats", id] });
    });

    return () => {
      unlisten.then((f) => f());
    };
  }, [account, id]);

  useEffect(() => {
    if (!data?.length) return;
    if (!ref.current) return;
    if (!shouldStickToBottom.current) return;

    ref.current.scrollToIndex(data.length - 1, {
      align: "end",
    });
  }, [data]);

  return (
    <ScrollArea.Root
      type={"scroll"}
      scrollHideDelay={300}
      className="overflow-hidden flex-1 w-full"
    >
      <ScrollArea.Viewport
        ref={scrollRef}
        className="relative h-full py-2 [&>div]:!flex [&>div]:flex-col [&>div]:justify-end"
      >
        <Virtualizer
          scrollRef={scrollRef as unknown as RefObject<HTMLElement>}
          ref={ref}
          shift={true}
          onScroll={(offset) => {
            if (!ref.current) return;
            shouldStickToBottom.current =
              offset - ref.current.scrollSize + ref.current.viewportSize >=
              -1.5;
          }}
        >
          {isLoading ? (
            <>
              <div className="flex items-center gap-3 my-1.5 px-3">
                <div className="flex-1 min-w-0 inline-flex">
                  <div className="w-44 h-[35px] py-2 max-w-[400px] bg-neutral-100 dark:bg-neutral-800 animate-pulse rounded-tl-3xl rounded-tr-3xl rounded-br-3xl rounded-bl-md" />
                </div>
              </div>
              <div className="flex items-center gap-3 my-1.5 px-3">
                <div className="flex-1 min-w-0 inline-flex justify-end">
                  <div className="w-44 h-[35px] py-2 max-w-[400px] bg-blue-500 text-white animate-pulse rounded-tl-3xl rounded-tr-3xl rounded-br-md rounded-bl-3xl" />
                </div>
              </div>
            </>
          ) : isError ? (
            <div className="w-full h-56 flex items-center justify-center">
              <div className="text-sm flex items-center gap-1.5">
                Cannot load message. Please try again later.
              </div>
            </div>
          ) : !data?.length ? (
            <div className="h-20 flex items-center justify-center">
              <CoopIcon className="size-10 text-neutral-200 dark:text-neutral-800" />
            </div>
          ) : (
            data?.map((item) => (
              <div
                key={item[0]}
                className="w-full flex flex-col items-center mt-3 gap-3"
              >
                <div className="text-xs text-center text-neutral-600 dark:text-neutral-400">
                  {item[0]}
                </div>
                <div className="w-full">
                  {item[1]
                    ? item[1]
                        .sort((a, b) => a.created_at - b.created_at)
                        .map((item, idx) => renderItem(item, idx))
                    : null}
                </div>
              </div>
            ))
          )}
        </Virtualizer>
      </ScrollArea.Viewport>
      <ScrollArea.Scrollbar
        className="flex select-none touch-none p-0.5 duration-[160ms] ease-out data-[orientation=vertical]:w-2"
        orientation="vertical"
      >
        <ScrollArea.Thumb className="flex-1 bg-black/40 dark:bg-white/40 rounded-full relative before:content-[''] before:absolute before:top-1/2 before:left-1/2 before:-translate-x-1/2 before:-translate-y-1/2 before:w-full before:h-full before:min-w-[44px] before:min-h-[44px]" />
      </ScrollArea.Scrollbar>
      <ScrollArea.Corner className="bg-transparent" />
    </ScrollArea.Root>
  );
}

function Message({ text }: { text: string }) {
  const delimiter =
    /((?:https?:\/\/)?(?:(?:[a-z0-9]?(?:[a-z0-9\-]{1,61}[a-z0-9])?\.[^\.|\s])+[a-z\.]*[a-z]+|(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)(?:\.(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)){3})(?::\d{1,5})*[a-z0-9.,_\/~#&=;%+?\-\\(\\)]*)/gi;

  return (
    <>
      {text.split(delimiter).map((word) => {
        const match = word.match(delimiter);
        if (match) {
          const url = match[0];
          return (
            <a
              href={url.startsWith("http") ? url : `http://${url}`}
              target="_blank"
              rel="noreferrer"
              className="underline"
            >
              {url}
            </a>
          );
        }
        return word;
      })}
    </>
  );
}

function Form() {
  const { id } = Route.useParams();
  const inboxRelays = Route.useLoaderData();

  const [newMessage, setNewMessage] = useState("");
  const [attaches, setAttaches] = useState<string[]>([]);
  const [isPending, startTransition] = useTransition();

  const remove = (item: string) => {
    setAttaches((prev) => prev.filter((att) => att !== item));
  };

  const submit = () => {
    startTransition(async () => {
      if (!newMessage.length) return;

      const content = `${newMessage}\r\n${attaches.join("\r\n")}`;
      const res = await commands.sendMessage(id, content);

      if (res.status === "error") {
        await message(res.error, {
          title: "Send mesaage failed",
          kind: "error",
        });
        return;
      }

      setNewMessage("");
      setAttaches([]);
    });
  };

  return (
    <div className="shrink-0 flex items-center justify-center px-3.5">
      {!inboxRelays.length ? (
        <div className="text-xs">
          This user doesn't have inbox relays. You cannot send messages to them.
        </div>
      ) : (
        <div className="flex-1 flex flex-col justify-end">
          {attaches?.length ? (
            <div className="flex items-center gap-2">
              {attaches.map((item, index) => (
                <button
                  type="button"
                  key={item}
                  onClick={() => remove(item)}
                  className="relative"
                >
                  <img
                    src={item}
                    alt={`File ${index}`}
                    className="aspect-square w-16 object-cover rounded-lg outline outline-1 -outline-offset-1 outline-black/10 dark:outline-black/50"
                    loading="lazy"
                    decoding="async"
                  />
                  <span className="absolute -top-2 -right-2 size-4 flex items-center justify-center bg-neutral-100 dark:bg-neutral-900 rounded-full border border-neutral-200 dark:border-neutral-800">
                    <X className="size-2" />
                  </span>
                </button>
              ))}
            </div>
          ) : null}
          <div className="h-12 w-full flex items-center gap-2">
            <div className="inline-flex gap-1">
              <AttachMedia onUpload={setAttaches} />
            </div>
            <input
              placeholder="Message..."
              value={newMessage}
              onChange={(e) => setNewMessage(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") submit();
              }}
              className="flex-1 h-9 rounded-full px-3.5 bg-transparent border border-neutral-200 dark:border-neutral-800 focus:outline-none focus:border-blue-500 placeholder:text-neutral-400 dark:placeholder:text-neutral-600"
            />
            <button
              type="button"
              title="Send message"
              disabled={isPending}
              onClick={() => submit()}
              className="rounded-full size-9 inline-flex items-center justify-center bg-blue-300 hover:bg-blue-500 dark:bg-blue-700 dark:hover:bg-blue-800 text-white"
            >
              {isPending ? <Spinner /> : <ArrowUp className="size-5" />}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function AttachMedia({
  onUpload,
}: {
  onUpload: Dispatch<SetStateAction<string[]>>;
}) {
  const [isPending, startTransition] = useTransition();

  const attach = () => {
    startTransition(async () => {
      const file = await upload();

      if (file) {
        onUpload((prev) => [...prev, file]);
      } else {
        return;
      }
    });
  };

  return (
    <button
      type="button"
      title="Attach media"
      onClick={() => attach()}
      className="size-9 inline-flex items-center justify-center hover:bg-neutral-100 dark:hover:bg-neutral-800 rounded-full"
    >
      {isPending ? (
        <Spinner className="size-4" />
      ) : (
        <Paperclip className="size-5" />
      )}
    </button>
  );
}
