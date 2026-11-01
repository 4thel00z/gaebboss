# GAEB DA XML schema files

The official GAEB DA XML schema files (XSD) published by the Gemeinsamer
Ausschuss Elektronik im Bauwesen (GAEB), vendored unmodified so that
`cargo xtask gen` can derive the typed model, readers, writers and phase
profiles from them and so the tests can check files against them.

- Source: https://www.gaeb.de/de/service/downloads/gaeb-datenaustausch/
  (public download, no registration)
- Downloaded: 2026-11-01
- Copyright: GAEB / DIN Deutsches Institut für Normung e. V. All rights
  remain with the rights holders. The files are not covered by this
  repository's license, and GAEB asks that schema files are never modified.
  None are.
- The Fachdokumentation PDFs are not vendored; download them from the same
  page.

| directory | package |
|---|---|
| `3.1/` | GAEB DA XML 3.1 2009-12: one schema for X81-X83 and X85-X87, one for X84, plus the W3C `xml.xsd` and `xmldsig-core-schema.xsd` they import |
| `3.2/` | GAEB DA XML 3.2 2013-10, Leistungsverzeichnis (X80-X87) |
| `3.3/` | GAEB DA XML 3.3 2021-05 (all packages) and the 2023-01 Mengenermittlung (X31) |

## Checksums (SHA-256)

| file | sha256 |
|---|---|
| `3.1/GAEB_DA_XML 3.1 2009-12.xsd` | `4baa2b88e489ca9797ad63e8e6ee5af6fd2d93a312bd9b9a8b6ad660a48438c3` |
| `3.1/GAEB_DA_XML_84 3.1 2009-12.xsd` | `f26d50c58c478fd7599c2a1f87a5d30451dce538ced3a4292c05e4e4200611de` |
| `3.1/xml.xsd` | `068d7ed95badc1ccf1be78b47e97a3ad9f7d10e17d3d3d684606e62a85de20ed` |
| `3.1/xmldsig-core-schema.xsd` | `af234e51dee3dbc3be288c79abdca95208e228f400259c8a485099bfb75eb88f` |
| `3.2/GAEB_DA_XML_80_3.2_2013-10.xsd` | `8a18d58d77cae60b1fcf04c2c0045cf05d1d132f039d369c70d40a2b07728559` |
| `3.2/GAEB_DA_XML_81_3.2_2013-10.xsd` | `655f8a49716efd7186f6598c29b5208032060f57bc7f251c68d895ff0a7213f5` |
| `3.2/GAEB_DA_XML_82_3.2_2013-10.xsd` | `b4b87c159b5b7b6b58b36100415345ab2276776c3e76fcb5d41a33943f860434` |
| `3.2/GAEB_DA_XML_83_3.2_2013-10.xsd` | `51bcd0cc37c7851dbb89520d8a6663b9fcaa9982575476410432e346132c8f6e` |
| `3.2/GAEB_DA_XML_84_3.2_2013-10.xsd` | `c57677b3b8be38371dabc9a83781049be483f0bd99eb9b771851ea9a6959ea5e` |
| `3.2/GAEB_DA_XML_85_3.2_2013-10.xsd` | `59b7d168e09dd3f0eb0c5469557f3b80e6b33948a87e5f49a9f916c3f8c047fe` |
| `3.2/GAEB_DA_XML_86_3.2_2013-10.xsd` | `41e4da0a21f4164e625d867f5c0dc5a5c49be3075689668ee3e364b95cf75706` |
| `3.2/GAEB_DA_XML_87_3.2_2013-10.xsd` | `088965bb60676f77a40b8cb68fbbdb8ab4bc234fc9faf1e5059531422cd808fc` |
| `3.2/GAEB_DA_XML_Lib_3.2_2013-10.xsd` | `f8d736e96f3f2e1ba6e97aa0492dedad1c2064fafc502a7f70466b34b67d1f77` |
| `3.3/GAEB_DA_XML_31_3.3_2023-01.xsd` | `179b9d8c9a93145af63298270e44c2d1ae820bb850edd0ba284d5b7cbef01b56` |
| `3.3/GAEB_DA_XML_50.1_3.3_2021-05.xsd` | `3003517f9821e46095e9c780ef68014242aaa0c476961be78102237aaf47d961` |
| `3.3/GAEB_DA_XML_50.2_3.3_2021-05.xsd` | `968c389e9827d59b1305a390185f4741f586c5c96c706093b5a0e01b3578e662` |
| `3.3/GAEB_DA_XML_50_3.3_2021-05.xsd` | `ccde2540af0b735e8d6196eb3525bddf972b6d40a776775e0aa3570ce5c1719c` |
| `3.3/GAEB_DA_XML_51.1_3.3_2021-05.xsd` | `3b75db60ef5bce123c1c3143bf0fd930dfeab7e3cb04c8087e4c77c0b8dedaf5` |
| `3.3/GAEB_DA_XML_51.2_3.3_2021-05.xsd` | `6b60e9c0832a8d2bebaf182754500e54f37f8b0ed01b0209f049cc68be5f6f37` |
| `3.3/GAEB_DA_XML_51_3.3_2021-05.xsd` | `868b9a1c1048f1e99ef2dfe9ec6576d9626521181ee685603158a12406a79467` |
| `3.3/GAEB_DA_XML_52_3.3_2021-05.xsd` | `e35484e4e9c042430885476cdafce78b4e4021a8ee6e2a4e63baf2fb422855c1` |
| `3.3/GAEB_DA_XML_61_3.3_2021-05_Beta.xsd` | `a4899749ae71975364e92407824f4da6f37f9ec71d6e3f4bbc2fcfd2eec9928d` |
| `3.3/GAEB_DA_XML_80_3.3_2021-05.xsd` | `d8b1bda0a48725838aabae34333a2dfcf48edf3c05c764575e32622416bd2ff4` |
| `3.3/GAEB_DA_XML_81_3.3_2021-05.xsd` | `aa24ed698088308d38f1ff7612643e55e70a847f6b5d433aa10dcd6dfe6c3ea3` |
| `3.3/GAEB_DA_XML_82_3.3_2021-05.xsd` | `6fcf94f41aeac0a060d29251ca085e29aa659b1c6b2d4872a76788cc7512c52b` |
| `3.3/GAEB_DA_XML_83Z_3.3_2021-05.xsd` | `9d240ab53de426f0ceb5c31f508bc65e30f0f5133997038d2f1e48b6b60ffa6d` |
| `3.3/GAEB_DA_XML_83_3.3_2021-05.xsd` | `a963ca5b6a93ee0f7a83d3138272a4116635e30e7b288c6ba8761ed444cabaa8` |
| `3.3/GAEB_DA_XML_84P_3.3_2021-05_Beta.xsd` | `6dc7015068b5dd2b62ad29b0ab09795f9182c495df5af5debf6fc0a002057f36` |
| `3.3/GAEB_DA_XML_84Z_3.3_2021-05.xsd` | `ae58260cdbd8178b933827b66c82290e5ab15a2042b460d0069a616cfdf088fe` |
| `3.3/GAEB_DA_XML_84_3.3_2021-05.xsd` | `67282b688db3bb12a4ded22c7b677e30ce105f9d1f80d02d084be67156e1cc42` |
| `3.3/GAEB_DA_XML_85_3.3_2021-05.xsd` | `ca8661aa7c1202105e3cf87385880b9d1741664d43fa92c6dc209a5cfe3d91cb` |
| `3.3/GAEB_DA_XML_86ZE_3.3_2021-05.xsd` | `20cb0be3323e7d80ecc9d491ec4e4e6c25d1589b1bfb1960a22bc0c3098ac0c3` |
| `3.3/GAEB_DA_XML_86ZR_3.3_2021-05.xsd` | `31609b39c683cb46be8f85256b95113efcacc86db9e7a7be1b610e4a9471c9e5` |
| `3.3/GAEB_DA_XML_86_3.3_2021-05.xsd` | `334853ca68fe07c1321880c40f27b776c8133ec8c34207c63b0d438ab6e45e0d` |
| `3.3/GAEB_DA_XML_87_3.3_2021-05.xsd` | `1f351ad4e6e4d9db9175c51f24d22b2808adc14dbb0abd0858a3cca59389f125` |
| `3.3/GAEB_DA_XML_89B_3.3_2021-05.xsd` | `4055c4820deaf2e9431258cdba35dfb878480f69c14d5b8c52ff184915cbeac6` |
| `3.3/GAEB_DA_XML_89_3.3_2021-05.xsd` | `2b3a8e3fdff708980b8feeee312d9b9d81131eccbaab1ab8442280b6f8ab6b87` |
| `3.3/GAEB_DA_XML_93_3.3_2021-05.xsd` | `1ac02c1248d63688cf6a8d0f59c3f7b3594743c6416b9a038822568a47740496` |
| `3.3/GAEB_DA_XML_94_3.3_2021-05.xsd` | `2d33bb36b10b775dff511fb82cf864ec19e37a087a366d26883aede1534a4c27` |
| `3.3/GAEB_DA_XML_96_3.3_2021-05.xsd` | `f981e69f16a41cf9205e5620f26b65cf77931dc8a3ac4f036f7698484681f99f` |
| `3.3/GAEB_DA_XML_97_3.3_2021-05.xsd` | `da0a29ef4e09a43b751d34d8b249faaf44bb1b744ca84ede00676c03f71fddb3` |
| `3.3/GAEB_DA_XML_98_3.3_2021-05_Beta.xsd` | `7ab63b5b6a56d0747b719b52836e2f0f9914913533d7a999dd41f739c5c58a5c` |
| `3.3/GAEB_DA_XML_99_3.3_2021-05_Beta.xsd` | `22b100b7bca2abbd0d6047cbe27c94f85cf8e0be41e2c94672df76dc7d9b36ee` |
| `3.3/GAEB_DA_XML_Lib5x_3.3_2021-05.xsd` | `01480fc38c77fca4313eea997c68e8a26ca8f84eccc7127b34c2bac7ae053368` |
| `3.3/GAEB_DA_XML_Lib_3.3_2021-05.xsd` | `a00cad3a5bfaf079de936393c0827c2387d4b9407c113359cca4b493903388ec` |
