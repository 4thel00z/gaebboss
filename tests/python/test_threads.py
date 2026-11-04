import concurrent.futures
import pathlib
import threading
import time
from collections.abc import Callable
from decimal import Decimal

import pytest

import gaebboss

BIDDER = {"name1": "Bau GmbH", "street": "Weg 1", "pcode": "12345", "city": "Berlin"}


def test_parsing_in_threads_releases_the_gil(x83: pathlib.Path) -> None:
    data = x83.read_bytes()
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        counts = list(pool.map(lambda _: len(gaebboss.Document(data=data).items()), range(16)))
    assert counts == [counts[0]] * 16


@pytest.fixture(scope="session")
def big_x83(tmp_path_factory: pytest.TempPathFactory) -> pathlib.Path:
    fixture = pathlib.Path(__file__).resolve().parents[1] / "fixtures" / "official" / "3.3"
    text = (fixture / "bvbs_bauausfuehrung.x83").read_text(encoding="utf-8")
    head, rest = text.split("<BoQBody>", 1)
    body, tail = rest.rsplit("</BoQBody>", 1)
    copies = [
        body.replace('RNoPart="', f'RNoPart="{n}').replace('ID="', f'ID="c{n}') for n in range(1, 110)
    ]
    path = tmp_path_factory.mktemp("big") / "big.X83"
    path.write_text(f"{head}<BoQBody>{''.join(copies)}</BoQBody>{tail}", encoding="utf-8")
    return path


def ticks_while(call: Callable[[], object]) -> int:
    ticks = 0
    stop = threading.Event()

    def ticker() -> None:
        nonlocal ticks
        while not stop.is_set():
            ticks += 1
            time.sleep(0.001)

    thread = threading.Thread(target=ticker)
    thread.start()
    try:
        deadline = time.monotonic() + 0.3
        while time.monotonic() < deadline:
            call()
    finally:
        stop.set()
        thread.join()
    return ticks


def calls(big_x83: pathlib.Path) -> dict[str, Callable[[], object]]:
    tender = gaebboss.Document(big_x83)
    bid = gaebboss.Bid.from_tender(tender, BIDDER)
    for item in tender.items():
        bid.price(item.id, Decimal("12.50"))
    x84 = bid.finish()
    return {
        "open": lambda: gaebboss.Document(big_x83),
        "items": tender.items,
        "entries": tender.entries,
        "validate": tender.validate,
        "validate_against": lambda: x84.validate_against(tender),
        "write_report": tender.write_report,
        "from_tender": lambda: gaebboss.Bid.from_tender(tender, BIDDER),
    }


@pytest.mark.parametrize(
    "name",
    ["open", "items", "entries", "validate", "validate_against", "write_report", "from_tender"],
)
def test_calls_release_the_gil(big_x83: pathlib.Path, name: str) -> None:
    call = calls(big_x83)[name]
    call()
    idle = ticks_while(lambda: time.sleep(0.005))
    busy = ticks_while(call)
    assert busy >= idle // 2, f"{name}: {busy} ticks, {idle} when idle"


def best_of_three(argument: object) -> float:
    def once() -> float:
        started = time.perf_counter()
        gaebboss.Document(data=argument, name="big.X83")
        return time.perf_counter() - started

    return min(once() for _ in range(3))


@pytest.mark.parametrize("wrap", [bytes, bytearray, memoryview])
def test_buffer_arguments_copy_in_one_go(
    big_x83: pathlib.Path, wrap: Callable[[bytes], object]
) -> None:
    data = big_x83.read_bytes()
    argument = wrap(data)
    reference = gaebboss.Document(data=data, name="big.X83")
    doc = gaebboss.Document(data=argument, name="big.X83")
    assert len(doc.items()) == len(reference.items())
    assert best_of_three(argument) < 2 * best_of_three(data) + 0.01
