#!/usr/bin/env python3
"""Fetch YouTube transcript without external dependencies.

Usage: youtube-transcript.py <video_id>
Output: JSON array to stdout
"""
import urllib.request, json, re, html as htmlmod, sys

def get_transcript(vid):
    url = f"https://www.youtube.com/watch?v={vid}"
    headers = {
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        "Accept-Language": "en,ja;q=0.9",
    }
    req = urllib.request.Request(url, headers=headers)
    page = urllib.request.urlopen(req, timeout=15).read().decode("utf-8")

    # Extract caption tracks
    m = re.search(r'"captionTracks":\s*(\[.*?\])', page)
    if not m:
        return None, "no caption tracks found"

    tracks = json.loads(m.group(1))
    if not tracks:
        return None, "empty caption tracks"

    # Prefer English, then any
    track = tracks[0]
    for t in tracks:
        if t.get("languageCode", "") == "en":
            track = t
            break

    track_url = track["baseUrl"]

    # Try fmt=json3 first
    for fmt in ["json3", "srv3", ""]:
        try:
            fetch_url = track_url + (f"&fmt={fmt}" if fmt else "")
            req2 = urllib.request.Request(fetch_url, headers=headers)
            data = urllib.request.urlopen(req2, timeout=10).read().decode("utf-8")
            if not data:
                continue

            if fmt == "json3":
                j = json.loads(data)
                texts = []
                for event in j.get("events", []):
                    for seg in event.get("segs", []):
                        t = seg.get("utf8", "").strip()
                        if t and t != "\n":
                            texts.append(t)
                if texts:
                    return " ".join(texts), None

            elif fmt == "srv3" or fmt == "":
                # XML format
                texts = re.findall(r'<text[^>]*>(.*?)</text>', data, re.S)
                if texts:
                    return " ".join(htmlmod.unescape(t).strip() for t in texts if t.strip()), None
        except Exception as e:
            continue

    # Fallback: try innertube get_transcript API
    try:
        # Extract params for transcript
        tp = re.search(r'"params":"([^"]+)"[^}]*"targetId":"engagement-panel-searchable-transcript"', page)
        if tp:
            params = tp.group(1)
            innertube_body = json.dumps({
                "context": {
                    "client": {
                        "clientName": "WEB",
                        "clientVersion": "2.20260101.00.00"
                    }
                },
                "params": params
            }).encode()
            innertube_req = urllib.request.Request(
                "https://www.youtube.com/youtubei/v1/get_transcript?prettyPrint=false",
                data=innertube_body,
                headers={**headers, "Content-Type": "application/json"},
            )
            innertube_resp = urllib.request.urlopen(innertube_req, timeout=10).read().decode("utf-8")
            innertube_data = json.loads(innertube_resp)
            # Extract text segments
            actions = innertube_data.get("actions", [])
            texts = []
            for action in actions:
                panel = action.get("updateEngagementPanelAction", {}).get("content", {})
                body = panel.get("transcriptRenderer", {}).get("body", {})
                sections = body.get("transcriptBodyRenderer", {}).get("cueGroups", [])
                for section in sections:
                    cues = section.get("transcriptCueGroupRenderer", {}).get("cues", [])
                    for cue in cues:
                        text = cue.get("transcriptCueRenderer", {}).get("cue", {}).get("simpleText", "")
                        if text.strip():
                            texts.append(text.strip())
            if texts:
                return " ".join(texts), None
    except Exception:
        pass

    return None, "all methods failed"


def get_metadata(vid):
    """Get video metadata via oEmbed API."""
    try:
        oembed_url = f"https://www.youtube.com/oembed?url=https://www.youtube.com/watch?v={vid}&format=json"
        req = urllib.request.Request(oembed_url, headers={"User-Agent": "Mozilla/5.0"})
        data = json.loads(urllib.request.urlopen(req, timeout=10).read().decode("utf-8"))
        return data.get("title", "untitled"), data.get("author_name", "")
    except Exception:
        return "untitled", ""


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: youtube-transcript.py <video_id>", file=sys.stderr)
        sys.exit(1)

    vid = sys.argv[1]
    title, author = get_metadata(vid)
    transcript, error = get_transcript(vid)

    result = [{
        "title": title,
        "author": author,
        "url": f"https://www.youtube.com/watch?v={vid}",
        "content": transcript or f"(transcript unavailable: {error})",
        "type": "youtube"
    }]
    json.dump(result, sys.stdout, ensure_ascii=False, indent=2)
