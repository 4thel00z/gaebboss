import pathlib

import pytest

FIXTURES = pathlib.Path(__file__).resolve().parents[1] / "fixtures" / "official"


@pytest.fixture
def x83() -> pathlib.Path:
    return FIXTURES / "3.3" / "bvbs_bauausfuehrung.x83"


@pytest.fixture
def x84() -> pathlib.Path:
    return FIXTURES / "3.3" / "bvbs_bauausfuehrung.x84"


@pytest.fixture
def musterdatei_x83() -> pathlib.Path:
    return FIXTURES / "3.1" / "Musterdatei_3.1_2008-001.X83"
