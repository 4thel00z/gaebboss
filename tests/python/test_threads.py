import concurrent.futures

import gaebboss


def test_parsing_in_threads_releases_the_gil(x83) -> None:
    data = x83.read_bytes()
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        counts = list(pool.map(lambda _: len(gaebboss.Document(data=data).items()), range(16)))
    assert counts == [counts[0]] * 16
