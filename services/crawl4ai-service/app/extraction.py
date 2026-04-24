import re
from typing import Tuple

from bs4 import BeautifulSoup, Comment


NON_VISIBLE_SELECTORS = [
    "script",
    "style",
    "template",
    "noscript",
    "svg",
    "canvas",
    "meta",
    "link",
    "iframe",
]

INJECTION_MARKER_PATTERNS = [
    re.compile(r"ignore\s+previous\s+instructions?", re.IGNORECASE),
    re.compile(r"respond\s+only\s+with\s+json", re.IGNORECASE),
    re.compile(r"\bsystem\s*:\s*", re.IGNORECASE),
    re.compile(r"you\s+are\s+now", re.IGNORECASE),
]


def extract_visible_text(html_source: str) -> Tuple[str, dict]:
    if not html_source:
        return "", {
            "extraction_mode": "strict_visible_only",
            "sanitization_nodes_removed": 0,
            "suspicious_marker_hits": 0,
        }

    soup = BeautifulSoup(html_source, "html.parser")
    nodes_removed = 0

    for selector in NON_VISIBLE_SELECTORS:
        for node in soup.select(selector):
            node.decompose()
            nodes_removed += 1

    for comment in soup.find_all(string=lambda text: isinstance(text, Comment)):
        comment.extract()
        nodes_removed += 1

    for node in list(soup.find_all(True)):
        if element_is_hidden(node):
            node.decompose()
            nodes_removed += 1

    visible_text = soup.get_text(" ", strip=True)
    normalized = re.sub(r"\s+", " ", visible_text).strip()
    suspicious_hits = count_suspicious_markers(normalized)
    return normalized, {
        "extraction_mode": "strict_visible_only",
        "sanitization_nodes_removed": nodes_removed,
        "suspicious_marker_hits": suspicious_hits,
    }


def element_is_hidden(node) -> bool:
    if node.has_attr("hidden"):
        return True

    aria_hidden = node.attrs.get("aria-hidden")
    if isinstance(aria_hidden, str) and aria_hidden.strip().lower() == "true":
        return True

    style = node.attrs.get("style")
    if not isinstance(style, str):
        return False

    normalized_style = style.lower().replace(" ", "")
    hidden_markers = [
        "display:none",
        "visibility:hidden",
        "opacity:0",
        "height:0",
        "width:0",
        "max-height:0",
        "max-width:0",
    ]
    return any(marker in normalized_style for marker in hidden_markers)


def count_suspicious_markers(text: str) -> int:
    if not text:
        return 0
    count = 0
    for pattern in INJECTION_MARKER_PATTERNS:
        count += len(pattern.findall(text))
    return count
