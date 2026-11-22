//! Vanilla `LanguageInfo` table, baked from Mojang lang files.

/// One entry per language.
macro_rules! languages {
    ($($variant:ident => $code:literal, $name:literal, $region:literal;)*) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum Language {
            $($variant,)*
        }

        impl Language {
            pub const ALL: &'static [Language] = &[$(Language::$variant,)*];

            pub fn code(self) -> &'static str {
                match self {
                    $(Language::$variant => $code,)*
                }
            }

            pub fn name(self) -> &'static str {
                match self {
                    $(Language::$variant => $name,)*
                }
            }

            pub fn region(self) -> &'static str {
                match self {
                    $(Language::$variant => $region,)*
                }
            }
        }
    };
}

languages! {
    EnUs => "en_us", "English", "US";
    AfZa => "af_za", "Afrikaans", "Suid-Afrika";
    ArSa => "ar_sa", "العربية", "العالم العربي";
    AstEs => "ast_es", "Asturianu", "Asturies";
    AzAz => "az_az", "Azərbaycanca", "Azərbaycan";
    BaRu => "ba_ru", "Башҡортса", "Рәсәй";
    Bar => "bar", "Boarisch", "Bayern";
    BeBy => "be_by", "Беларуская", "Беларусь";
    BgBg => "bg_bg", "Български", "България";
    BrFr => "br_fr", "Brezhoneg", "Breizh";
    Brb => "brb", "Braobans", "Braobant";
    BsBa => "bs_ba", "Bosanski", "Bosna i Hercegovina";
    CaEs => "ca_es", "Català", "Catalunya";
    CsCz => "cs_cz", "Čeština", "Česko";
    CyGb => "cy_gb", "Cymraeg", "Cymru";
    DaDk => "da_dk", "Dansk", "Danmark";
    DeAt => "de_at", "Deitsch", "Österreich";
    DeCh => "de_ch", "Schwiizerdütsch", "Schwiiz";
    DeDe => "de_de", "Deutsch", "Deutschland";
    ElGr => "el_gr", "Ελληνικά", "Ελλάδα";
    EnAu => "en_au", "English", "Australia";
    EnCa => "en_ca", "English", "Canada";
    EnGb => "en_gb", "English", "United Kingdom";
    EnNz => "en_nz", "English", "New Zealand";
    EnPt => "en_pt", "Pirate Speak", "The Seven Seas";
    EnUd => "en_ud", "ɥsᴉꞁᵷuƎ", "uʍoᗡ ǝpᴉsd∩";
    Enp => "enp", "Anglish", "Foroned Kingdom";
    Enws => "enws", "Shakespearean English", "Kingdom of England";
    EoUy => "eo_uy", "Esperanto", "Esperantujo";
    EsAr => "es_ar", "Español", "Argentina";
    EsCl => "es_cl", "Español", "Chile";
    EsEc => "es_ec", "Español", "Ecuador";
    EsEs => "es_es", "Español", "España";
    EsMx => "es_mx", "Español", "México";
    EsUy => "es_uy", "Español", "Uruguay";
    EsVe => "es_ve", "Español", "Venezuela";
    Esan => "esan", "Andalûh", "Andaluçía";
    EtEe => "et_ee", "Eesti keel", "Eesti";
    EuEs => "eu_es", "Euskara", "Euskal Herria";
    FaIr => "fa_ir", "فارسی", "ايران";
    FiFi => "fi_fi", "Suomi", "Suomi";
    FilPh => "fil_ph", "Filipino", "Pilipinas";
    FoFo => "fo_fo", "Føroyskt", "Føroyar";
    FrCa => "fr_ca", "Français", "Canada";
    FrFr => "fr_fr", "Français", "France";
    FraDe => "fra_de", "Fränggisch", "Franggn";
    FurIt => "fur_it", "Furlan", "Friûl";
    FyNl => "fy_nl", "Frysk", "Fryslân";
    GaIe => "ga_ie", "Gaeilge", "Éire";
    GdGb => "gd_gb", "Gàidhlig", "Alba";
    GlEs => "gl_es", "Galego", "Galicia / Galiza";
    HawUs => "haw_us", "ʻŌlelo Hawaiʻi", "Hawaiʻi";
    HeIl => "he_il", "עברית", "ישראל";
    HiIn => "hi_in", "हिंदी", "भारत";
    HrHr => "hr_hr", "Hrvatski", "Hrvatska";
    HuHu => "hu_hu", "Magyar", "Magyarország";
    HyAm => "hy_am", "Հայերեն", "Հայաստան";
    IdId => "id_id", "Bahasa Indonesia", "Indonesia";
    IgNg => "ig_ng", "Igbo", "Naigeria";
    IoEn => "io_en", "Ido", "Idia";
    IsIs => "is_is", "Íslenska", "Ísland";
    Isv => "isv", "Medžuslovjansky", "Slovjanščina";
    ItIt => "it_it", "Italiano", "Italia";
    JaJp => "ja_jp", "日本語", "日本";
    JboEn => "jbo_en", "la .lojban.", "la jbogu'e";
    KaGe => "ka_ge", "ქართული", "საქართველო";
    KkKz => "kk_kz", "Қазақша", "Қазақстан";
    KnIn => "kn_in", "ಕನ್ನಡ", "ಭಾರತ";
    KoKr => "ko_kr", "한국어", "대한민국";
    Ksh => "ksh", "Kölsch/Ripoarisch", "Rhingland";
    KwGb => "kw_gb", "Kernewek", "Kernow";
    LaLa => "la_la", "Latina", "Latium";
    LbLu => "lb_lu", "Lëtzebuergesch", "Lëtzebuerg";
    LiLi => "li_li", "Limburgs", "Limburg";
    Lmo => "lmo", "Lombard", "Lombardia";
    LoLa => "lo_la", "ລາວ", "ປະເທດລາວ";
    LolUs => "lol_us", "LOLCAT", "Kingdom of Cats";
    LtLt => "lt_lt", "Lietuvių", "Lietuva";
    LvLv => "lv_lv", "Latviešu", "Latvija";
    Lzh => "lzh", "文言", "華夏";
    MkMk => "mk_mk", "Македонски", "Македонија";
    MnMn => "mn_mn", "Монгол", "Монгол Улс";
    MsMy => "ms_my", "Bahasa Melayu", "Malaysia";
    MtMt => "mt_mt", "Malti", "Malta";
    Nah => "nah", "Mēxikatlahtōlli", "Wāxtēkapān";
    NdsDe => "nds_de", "Plattdüütsch", "Düütschland";
    NlBe => "nl_be", "Vlaams", "België";
    NlNl => "nl_nl", "Nederlands", "Nederland";
    NnNo => "nn_no", "Norsk", "Noreg";
    NoNo => "no_no", "Norsk bokmål", "Norge";
    OcFr => "oc_fr", "Occitan", "Occitània";
    Ovd => "ovd", "Övdalska", "Swerre";
    PlPl => "pl_pl", "Polski", "Polska";
    PtBr => "pt_br", "Português", "Brasil";
    PtPt => "pt_pt", "Português", "Portugal";
    QyaAa => "qya_aa", "Quenya", "Arda";
    RoRo => "ro_ro", "Română", "România";
    Rpr => "rpr", "Русскій дореформенный", "Россійская имперія";
    RuRu => "ru_ru", "Русский", "Россия";
    RyUa => "ry_ua", "Руснацькый", "Пудкарпатя, Украина";
    SahSah => "sah_sah", "Сахалыы", "Cаха Сирэ";
    SeNo => "se_no", "Davvisámegiella", "Sápmi";
    SkSk => "sk_sk", "Slovenčina", "Slovensko";
    SlSi => "sl_si", "Slovenščina", "Slovenija";
    SoSo => "so_so", "Soomaali", "Soomaaliya";
    SqAl => "sq_al", "Shqip", "Shqipëri";
    SrCs => "sr_cs", "Srpski", "Srbija";
    SrSp => "sr_sp", "Српски", "Србија";
    SvSe => "sv_se", "Svenska", "Sverige";
    Sxu => "sxu", "Säggs’sch", "Saggsn";
    Szl => "szl", "Ślōnskŏ", "Gōrny Ślōnsk";
    TaIn => "ta_in", "தமிழ்", "இந்தியா";
    ThTh => "th_th", "ไทย", "ประเทศไทย";
    TlPh => "tl_ph", "Tagalog", "Pilipinas";
    TlhAa => "tlh_aa", "tlhIngan Hol", "tlhIngan wo'";
    Tok => "tok", "toki pona", "ma pona";
    TrTr => "tr_tr", "Türkçe", "Türkiye";
    TtRu => "tt_ru", "Татарча", "Рәсәй";
    UkUa => "uk_ua", "Українська", "Україна";
    ValEs => "val_es", "Català (Valencià)", "País Valencià";
    VecIt => "vec_it", "Vèneto", "Veneto";
    ViVn => "vi_vn", "Tiếng Việt", "Việt Nam";
    YiDe => "yi_de", "ייִדיש", "אשכנזישע יידן";
    YoNg => "yo_ng", "Yorùbá", "Nàìjíríà";
    ZhCn => "zh_cn", "简体中文", "中国大陆";
    ZhHk => "zh_hk", "繁體中文", "香港特別行政區";
    ZhTw => "zh_tw", "繁體中文", "台灣";
    ZlmArab => "zlm_arab", "بهاس ملايو", "مليسيا";
}

impl Default for Language {
    /// Vanilla `Options.languageCode = "en_us"`.
    fn default() -> Self {
        Language::EnUs
    }
}

impl Language {
    /// Filename stem to `Language`, for `MELON_LANG` / `options.txt`.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|l| l.code() == code)
    }
}
