"""TikTok Live -> Strata Graphics.

TikTok has no API for the comments of a live: this bridge reads them with the
open-source TikTokLive library and posts them to the server's webhook of a
TikTok connection (main menu > Data > Social networks), as
{id, author: {id, name, handle, avatar}, text, amount}. While it runs it also
posts {"heartbeat": true} every 30 s: the connection shows "connected" as long
as the bridge called in the last minute.

    python bridge.py @account "https://server/api/social/hook/<id>?token=<token>"
"""

import asyncio
import json
import sys
import time
import urllib.request
import uuid

from TikTokLive import TikTokLiveClient
from TikTokLive.events import CommentEvent, ConnectEvent, DisconnectEvent, GiftEvent

if len(sys.argv) != 3:
    sys.exit(__doc__)
ACCOUNT, HOOK = sys.argv[1], sys.argv[2]


def post(body):
    req = urllib.request.Request(HOOK, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
    try:
        urllib.request.urlopen(req, timeout=10).read()
    except Exception as e:  # The server restarting: the next ones go through.
        print(f"cannot post to Strata: {e}", file=sys.stderr)


async def send(body):
    await asyncio.get_running_loop().run_in_executor(None, post, body)


def author(user):
    thumb = getattr(user, "avatar_thumb", None)
    urls = getattr(thumb, "m_urls", None) or getattr(thumb, "url_list", None) or []
    handle = getattr(user, "unique_id", "") or getattr(user, "display_id", "") or ""
    return {
        "id": str(getattr(user, "id", "") or handle),
        "name": getattr(user, "nickname", "") or handle,
        "handle": handle,
        "avatar": urls[0] if urls else "",
    }


def message_id(event):
    common = getattr(event, "common", None) or getattr(event, "base_message", None)
    mid = getattr(common, "msg_id", None) or getattr(common, "message_id", None)
    return str(mid) if mid else uuid.uuid4().hex


client = TikTokLiveClient(unique_id=ACCOUNT)
alive = False


@client.on(ConnectEvent)
async def on_connect(event):
    global alive
    alive = True
    print(f"connected to the live of {ACCOUNT}")
    await send({"heartbeat": True})


@client.on(DisconnectEvent)
async def on_disconnect(event):
    global alive
    alive = False


@client.on(CommentEvent)
async def on_comment(event):
    await send({"id": message_id(event), "author": author(event.user), "text": event.comment})


@client.on(GiftEvent)
async def on_gift(event):
    # A gift sent in a streak is counted once, at its end.
    if event.gift.streakable and getattr(event, "streaking", False):
        return
    amount = f"{event.gift.name} × {event.repeat_count}"
    await send({"id": message_id(event), "author": author(event.user), "text": "", "amount": amount})


async def heartbeat():
    while True:
        await asyncio.sleep(30)
        if alive:
            await send({"heartbeat": True})


async def main():
    asyncio.create_task(heartbeat())
    while True:
        try:
            await client.connect()
        except Exception as e:  # Not live (yet), or the connection dropped.
            print(f"{ACCOUNT}: {e}; trying again in 60 s", file=sys.stderr)
        await asyncio.sleep(60)


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        pass
