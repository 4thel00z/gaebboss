from collections.abc import Buffer
from decimal import Decimal
from os import PathLike
from typing import Literal

__version__: str

class GaebError(Exception): ...

class Warning:
    code: str
    offset: int
    message: str

class Project:
    name: str | None
    label: str | None
    currency: str | None

class Level:
    type: str
    label: str | None
    length: int
    numeric: bool
    alignment: str | None

class Boq:
    id: str | None
    name: str | None
    label: str | None
    breakdown: list[Level]
    total: Decimal | None

class Complement:
    mark: int | None
    kind: str
    label: str
    value: str
    empty: bool
    tail: str
    number: Decimal | None

class Item:
    id: str
    oz: str
    kind: str
    alternative_group: int | None
    alternative_serial: int | None
    short_text: str
    long_text: str
    long_text_markdown: str
    quantity: Decimal | None
    unit: str | None
    unit_price: Decimal | None
    total: Decimal | None
    vat: Decimal | None
    counts_towards_total: bool
    not_offered: bool
    category_ozs: list[str]
    complements: list[Complement]

class Category:
    oz: str
    label: str
    depth: int
    total: Decimal | None

class MarkupItem:
    id: str
    oz: str
    base: Decimal | None
    percent: Decimal | None
    total: Decimal | None

class Finding:
    severity: str
    code: str
    oz: str | None
    message: str

class Dropped:
    type_name: str
    element: str
    count: int

class Missing:
    type_name: str
    element: str

class WriteReport:
    dropped: list[Dropped]
    missing: list[Missing]

class Document:
    def __init__(self, path: str | PathLike[str] | None = None, *, data: Buffer | None = None, name: str | None = None) -> None: ...
    @property
    def version(self) -> str: ...
    @property
    def phase(self) -> str | None: ...
    @property
    def warnings(self) -> list[Warning]: ...
    @property
    def project(self) -> Project: ...
    @property
    def boq(self) -> Boq | None: ...
    def items(self, kind: Literal["all", "counting"] = "all") -> list[Item]: ...
    def categories(self) -> list[Category]: ...
    def markup_items(self) -> list[MarkupItem]: ...
    def to_json(self) -> str: ...
    def validate(self) -> list[Finding]: ...
    def validate_against(self, tender: Document) -> list[Finding]: ...
    def write(self, *, phase: str | None = None, version: str | None = None, pretty: bool = False, strict: bool = False) -> bytes: ...
    def write_report(self, *, phase: str | None = None, version: str | None = None, strict: bool = False) -> WriteReport: ...

class Bid:
    @staticmethod
    def from_tender(tender: Document, bidder: dict[str, str]) -> Bid: ...
    def price(self, id: str, up: Decimal | str | int) -> None: ...
    def price_components(self, id: str, components: list[Decimal | str | int]) -> None: ...
    def not_offered(self, id: str) -> None: ...
    def free_quantity(self, id: str, qty: Decimal | str | int) -> None: ...
    def fill_complement(self, id: str, mark: int, text: str) -> None: ...
    def markup(self, id: str, base: Decimal | str | int, percent: Decimal | str | int) -> None: ...
    def discount_percent(self, percent: Decimal | str | int) -> None: ...
    def vat_percent(self, percent: Decimal | str | int) -> None: ...
    def finish(self) -> Document: ...
