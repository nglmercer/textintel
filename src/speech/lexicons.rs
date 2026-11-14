//! Localized month names and unit/currency patterns from Unicode CLDR.
//! Source: <https://github.com/unicode-org/cldr-json> (retrieved 2026-11-13).
//! Only the small speech-normalization subset is bundled; no runtime download.
/*
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 1991-2026 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

*/
use super::types::SpeechLocale;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::OnceLock;

fn data() -> &'static BTreeMap<String, Value> {
    static DATA: OnceLock<BTreeMap<String, Value>> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(LEXICONS).expect("bundled CLDR speech lexicons must be valid")
    })
}

pub(super) fn month(locale: SpeechLocale, month: u32) -> Option<&'static str> {
    data()
        .get(locale.code())?
        .get("months")?
        .get(month.checked_sub(1)? as usize)?
        .as_str()
}

pub(super) fn unit_pattern(
    locale: SpeechLocale,
    key: &str,
    category: &str,
) -> Option<(&'static str, Option<&'static str>)> {
    let value = data().get(locale.code())?.get("units")?.get(key)?;
    let key = format!("unitPattern-count-{category}");
    let pattern = value
        .get(&key)
        .or_else(|| value.get("unitPattern-count-other"))?
        .as_str()?;
    Some((pattern, value.get("gender").and_then(Value::as_str)))
}

pub(super) fn currency_name(
    locale: SpeechLocale,
    code: &str,
    category: &str,
) -> Option<&'static str> {
    let value = data().get(locale.code())?.get("currencies")?.get(code)?;
    let key = format!("displayName-count-{category}");
    value
        .get(&key)
        .or_else(|| value.get("displayName-count-other"))?
        .as_str()
}

const LEXICONS: &str = r#"{
  "en": {
    "date_pattern": "MMMM d, y",
    "months": [
      "January",
      "February",
      "March",
      "April",
      "May",
      "June",
      "July",
      "August",
      "September",
      "October",
      "November",
      "December"
    ],
    "units": {
      "concentr-percent": {
        "unitPattern-count-one": "{0} percent",
        "unitPattern-count-other": "{0} percent"
      },
      "duration-hour": {
        "unitPattern-count-one": "{0} hour",
        "unitPattern-count-other": "{0} hours"
      },
      "duration-minute": {
        "unitPattern-count-one": "{0} minute",
        "unitPattern-count-other": "{0} minutes"
      },
      "duration-second": {
        "unitPattern-count-one": "{0} second",
        "unitPattern-count-other": "{0} seconds"
      },
      "length-meter": {
        "unitPattern-count-one": "{0} meter",
        "unitPattern-count-other": "{0} meters"
      },
      "length-kilometer": {
        "unitPattern-count-one": "{0} kilometer",
        "unitPattern-count-other": "{0} kilometers"
      },
      "mass-gram": {
        "unitPattern-count-one": "{0} gram",
        "unitPattern-count-other": "{0} grams"
      },
      "mass-kilogram": {
        "unitPattern-count-one": "{0} kilogram",
        "unitPattern-count-other": "{0} kilograms"
      },
      "temperature-celsius": {
        "unitPattern-count-one": "{0} degree Celsius",
        "unitPattern-count-other": "{0} degrees Celsius"
      },
      "volume-liter": {
        "unitPattern-count-one": "{0} liter",
        "unitPattern-count-other": "{0} liters"
      },
      "volume-milliliter": {
        "unitPattern-count-one": "{0} milliliter",
        "unitPattern-count-other": "{0} milliliters"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-one": "US dollar",
        "displayName-count-other": "US dollars"
      },
      "EUR": {
        "displayName-count-one": "euro",
        "displayName-count-other": "euros"
      },
      "GBP": {
        "displayName-count-one": "British pound",
        "displayName-count-other": "British pounds"
      },
      "PEN": {
        "displayName-count-one": "Peruvian sol",
        "displayName-count-other": "Peruvian soles"
      },
      "JPY": {
        "displayName-count-one": "Japanese yen",
        "displayName-count-other": "Japanese yen"
      },
      "CNY": {
        "displayName-count-one": "Chinese yuan",
        "displayName-count-other": "Chinese yuan"
      },
      "INR": {
        "displayName-count-one": "Indian rupee",
        "displayName-count-other": "Indian rupees"
      },
      "KRW": {
        "displayName-count-one": "South Korean won",
        "displayName-count-other": "South Korean won"
      },
      "RUB": {
        "displayName-count-one": "Russian ruble",
        "displayName-count-other": "Russian rubles"
      },
      "BRL": {
        "displayName-count-one": "Brazilian real",
        "displayName-count-other": "Brazilian reals"
      },
      "TRY": {
        "displayName-count-one": "Turkish lira",
        "displayName-count-other": "Turkish Lira"
      },
      "BDT": {
        "displayName-count-one": "Bangladeshi taka",
        "displayName-count-other": "Bangladeshi takas"
      },
      "PKR": {
        "displayName-count-one": "Pakistani rupee",
        "displayName-count-other": "Pakistani rupees"
      },
      "IDR": {
        "displayName-count-one": "Indonesian rupiah",
        "displayName-count-other": "Indonesian rupiahs"
      },
      "VND": {
        "displayName-count-one": "Vietnamese dong",
        "displayName-count-other": "Vietnamese dong"
      }
    }
  },
  "es": {
    "date_pattern": "d 'de' MMMM 'de' y",
    "months": [
      "enero",
      "febrero",
      "marzo",
      "abril",
      "mayo",
      "junio",
      "julio",
      "agosto",
      "septiembre",
      "octubre",
      "noviembre",
      "diciembre"
    ],
    "units": {
      "concentr-percent": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} por ciento",
        "unitPattern-count-other": "{0} por ciento"
      },
      "duration-hour": {
        "gender": "feminine",
        "unitPattern-count-one": "{0} hora",
        "unitPattern-count-other": "{0} horas"
      },
      "duration-minute": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} minuto",
        "unitPattern-count-other": "{0} minutos"
      },
      "duration-second": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} segundo",
        "unitPattern-count-other": "{0} segundos"
      },
      "length-meter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} metro",
        "unitPattern-count-other": "{0} metros"
      },
      "length-kilometer": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} kilómetro",
        "unitPattern-count-other": "{0} kilómetros"
      },
      "mass-gram": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} gramo",
        "unitPattern-count-other": "{0} gramos"
      },
      "mass-kilogram": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} kilogramo",
        "unitPattern-count-other": "{0} kilogramos"
      },
      "temperature-celsius": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} grado Celsius",
        "unitPattern-count-other": "{0} grados Celsius"
      },
      "volume-liter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} litro",
        "unitPattern-count-other": "{0} litros"
      },
      "volume-milliliter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} mililitro",
        "unitPattern-count-other": "{0} mililitros"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-one": "dólar estadounidense",
        "displayName-count-other": "dólares estadounidenses"
      },
      "EUR": {
        "displayName-count-one": "euro",
        "displayName-count-other": "euros"
      },
      "GBP": {
        "displayName-count-one": "libra esterlina",
        "displayName-count-other": "libras esterlinas"
      },
      "PEN": {
        "displayName-count-one": "sol peruano",
        "displayName-count-other": "soles peruanos"
      },
      "JPY": {
        "displayName-count-one": "yen japonés",
        "displayName-count-other": "yenes japoneses"
      },
      "CNY": {
        "displayName-count-one": "yuan renminbi",
        "displayName-count-other": "yuanes renminbi"
      },
      "INR": {
        "displayName-count-one": "rupia india",
        "displayName-count-other": "rupias indias"
      },
      "KRW": {
        "displayName-count-one": "won surcoreano",
        "displayName-count-other": "wons surcoreanos"
      },
      "RUB": {
        "displayName-count-one": "rublo ruso",
        "displayName-count-other": "rublos rusos"
      },
      "BRL": {
        "displayName-count-one": "real brasileño",
        "displayName-count-other": "reales brasileños"
      },
      "TRY": {
        "displayName-count-one": "lira turca",
        "displayName-count-other": "liras turcas"
      },
      "BDT": {
        "displayName-count-one": "taka bangladesí",
        "displayName-count-other": "takas bangladesíes"
      },
      "PKR": {
        "displayName-count-one": "rupia pakistaní",
        "displayName-count-other": "rupias pakistaníes"
      },
      "IDR": {
        "displayName-count-one": "rupia indonesia",
        "displayName-count-other": "rupias indonesias"
      },
      "VND": {
        "displayName-count-one": "dong vietnamita",
        "displayName-count-other": "dongs vietnamitas"
      }
    }
  },
  "zh": {
    "date_pattern": "y年M月d日",
    "months": [
      "一月",
      "二月",
      "三月",
      "四月",
      "五月",
      "六月",
      "七月",
      "八月",
      "九月",
      "十月",
      "十一月",
      "十二月"
    ],
    "units": {
      "concentr-percent": {
        "unitPattern-count-other": "{0}%"
      },
      "duration-hour": {
        "unitPattern-count-other": "{0}小时"
      },
      "duration-minute": {
        "unitPattern-count-other": "{0}分钟"
      },
      "duration-second": {
        "unitPattern-count-other": "{0}秒钟"
      },
      "length-meter": {
        "unitPattern-count-other": "{0}米"
      },
      "length-kilometer": {
        "unitPattern-count-other": "{0}公里"
      },
      "mass-gram": {
        "unitPattern-count-other": "{0}克"
      },
      "mass-kilogram": {
        "unitPattern-count-other": "{0}千克"
      },
      "temperature-celsius": {
        "unitPattern-count-other": "{0}摄氏度"
      },
      "volume-liter": {
        "unitPattern-count-other": "{0}升"
      },
      "volume-milliliter": {
        "unitPattern-count-other": "{0}毫升"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-other": "美元"
      },
      "EUR": {
        "displayName-count-other": "欧元"
      },
      "GBP": {
        "displayName-count-other": "英镑"
      },
      "PEN": {
        "displayName-count-other": "秘鲁索尔"
      },
      "JPY": {
        "displayName-count-other": "日元"
      },
      "CNY": {
        "displayName-count-other": "人民币"
      },
      "INR": {
        "displayName-count-other": "印度卢比"
      },
      "KRW": {
        "displayName-count-other": "韩元"
      },
      "RUB": {
        "displayName-count-other": "俄罗斯卢布"
      },
      "BRL": {
        "displayName-count-other": "巴西雷亚尔"
      },
      "TRY": {
        "displayName-count-other": "土耳其里拉"
      },
      "BDT": {
        "displayName-count-other": "孟加拉塔卡"
      },
      "PKR": {
        "displayName-count-other": "巴基斯坦卢比"
      },
      "IDR": {
        "displayName-count-other": "印度尼西亚卢比"
      },
      "VND": {
        "displayName-count-other": "越南盾"
      }
    }
  },
  "hi": {
    "date_pattern": "d MMMM y",
    "months": [
      "जनवरी",
      "फ़रवरी",
      "मार्च",
      "अप्रैल",
      "मई",
      "जून",
      "जुलाई",
      "अगस्त",
      "सितंबर",
      "अक्टूबर",
      "नवंबर",
      "दिसंबर"
    ],
    "units": {
      "concentr-percent": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} प्रतिशत",
        "unitPattern-count-other": "{0} प्रतिशत"
      },
      "duration-hour": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} घंटा",
        "unitPattern-count-other": "{0} घंटे"
      },
      "duration-minute": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} मिनट",
        "unitPattern-count-other": "{0} मिनट"
      },
      "duration-second": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} सेकंड",
        "unitPattern-count-other": "{0} सेकंड"
      },
      "length-meter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} मीटर",
        "unitPattern-count-other": "{0} मीटर"
      },
      "length-kilometer": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} किलोमीटर",
        "unitPattern-count-other": "{0} किलोमीटर"
      },
      "mass-gram": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} ग्राम",
        "unitPattern-count-other": "{0} ग्राम"
      },
      "mass-kilogram": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} किलोग्राम",
        "unitPattern-count-other": "{0} किलोग्राम"
      },
      "temperature-celsius": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} डिग्री सेल्सियस",
        "unitPattern-count-other": "{0} डिग्री सेल्सियस"
      },
      "volume-liter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} लीटर",
        "unitPattern-count-other": "{0} लीटर"
      },
      "volume-milliliter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} मिलीलीटर",
        "unitPattern-count-other": "{0} मिलीलीटर"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-other": "यूएस डॉलर"
      },
      "EUR": {
        "displayName-count-other": "यूरो"
      },
      "GBP": {
        "displayName-count-other": "ब्रिटिश पाउंड स्टर्लिंग"
      },
      "PEN": {
        "displayName-count-other": "पेरूवियन सोल"
      },
      "JPY": {
        "displayName-count-other": "जापानी येन"
      },
      "CNY": {
        "displayName-count-other": "चीनी युआन"
      },
      "INR": {
        "displayName-count-one": "भारतीय रुपया",
        "displayName-count-other": "भारतीय रुपए"
      },
      "KRW": {
        "displayName-count-other": "दक्षिण कोरियाई वॉन"
      },
      "RUB": {
        "displayName-count-other": "रूसी रूबल"
      },
      "BRL": {
        "displayName-count-other": "ब्राज़ीली रियाल"
      },
      "TRY": {
        "displayName-count-other": "तुर्की लीरा"
      },
      "BDT": {
        "displayName-count-other": "बांग्लादेशी टका"
      },
      "PKR": {
        "displayName-count-one": "पाकिस्तानी रुपया",
        "displayName-count-other": "पाकिस्तानी रुपए"
      },
      "IDR": {
        "displayName-count-one": "इंडोनेशियाई रुपिया",
        "displayName-count-other": "इंडोनेशियाई रुपिये"
      },
      "VND": {
        "displayName-count-other": "वियतनामी डोंग"
      }
    }
  },
  "ar": {
    "date_pattern": "d MMMM y",
    "months": [
      "يناير",
      "فبراير",
      "مارس",
      "أبريل",
      "مايو",
      "يونيو",
      "يوليو",
      "أغسطس",
      "سبتمبر",
      "أكتوبر",
      "نوفمبر",
      "ديسمبر"
    ],
    "units": {
      "concentr-percent": {
        "gender": "feminine",
        "unitPattern-count-zero": "{0}٪",
        "unitPattern-count-one": "{0} بالمائة",
        "unitPattern-count-two": "{0}٪",
        "unitPattern-count-few": "{0}٪",
        "unitPattern-count-many": "{0}٪",
        "unitPattern-count-other": "{0} بالمائة"
      },
      "duration-hour": {
        "gender": "feminine",
        "unitPattern-count-zero": "{0} ساعة",
        "unitPattern-count-one": "ساعة",
        "unitPattern-count-two": "ساعتان",
        "unitPattern-count-few": "{0} ساعات",
        "unitPattern-count-many": "{0} ساعة",
        "unitPattern-count-other": "{0} ساعة"
      },
      "duration-minute": {
        "gender": "feminine",
        "unitPattern-count-zero": "{0} دقيقة",
        "unitPattern-count-one": "دقيقة",
        "unitPattern-count-two": "دقيقتان",
        "unitPattern-count-few": "{0} دقائق",
        "unitPattern-count-many": "{0} دقيقة",
        "unitPattern-count-other": "{0} دقيقة"
      },
      "duration-second": {
        "gender": "feminine",
        "unitPattern-count-zero": "{0} ثانية",
        "unitPattern-count-one": "ثانية",
        "unitPattern-count-two": "ثانيتان",
        "unitPattern-count-few": "{0} ثوان",
        "unitPattern-count-many": "{0} ثانية",
        "unitPattern-count-other": "{0} ثانية"
      },
      "length-meter": {
        "gender": "masculine",
        "unitPattern-count-zero": "{0} متر",
        "unitPattern-count-one": "متر",
        "unitPattern-count-two": "{0} متر",
        "unitPattern-count-few": "{0} أمتار",
        "unitPattern-count-many": "{0} مترًا",
        "unitPattern-count-other": "{0} متر"
      },
      "length-kilometer": {
        "gender": "masculine",
        "unitPattern-count-zero": "{0} كيلومتر",
        "unitPattern-count-one": "{0} كيلومتر",
        "unitPattern-count-two": "{0} كيلومتر",
        "unitPattern-count-few": "{0} كيلومترات",
        "unitPattern-count-many": "{0} كيلومترًا",
        "unitPattern-count-other": "{0} كيلومتر"
      },
      "mass-gram": {
        "gender": "masculine",
        "unitPattern-count-zero": "{0} غرام",
        "unitPattern-count-one": "غرام",
        "unitPattern-count-two": "غرامان",
        "unitPattern-count-few": "{0} غرامات",
        "unitPattern-count-many": "{0} غرامًا",
        "unitPattern-count-other": "{0} غرام"
      },
      "mass-kilogram": {
        "gender": "masculine",
        "unitPattern-count-zero": "{0} كيلوغرام",
        "unitPattern-count-one": "{0} كيلوغرام",
        "unitPattern-count-two": "{0} كيلوغرام",
        "unitPattern-count-few": "{0} كيلوغرام",
        "unitPattern-count-many": "{0} كيلوغرام",
        "unitPattern-count-other": "{0} كيلوغرام"
      },
      "temperature-celsius": {
        "gender": "feminine",
        "unitPattern-count-zero": "{0} درجة مئوية",
        "unitPattern-count-one": "{0} درجة مئوية",
        "unitPattern-count-two": "{0} درجة مئوية",
        "unitPattern-count-few": "{0} درجة مئوية",
        "unitPattern-count-many": "{0} درجة مئوية",
        "unitPattern-count-other": "{0} درجة مئوية"
      },
      "volume-liter": {
        "gender": "masculine",
        "unitPattern-count-zero": "{0} لتر",
        "unitPattern-count-one": "لتر",
        "unitPattern-count-two": "{0} لتر",
        "unitPattern-count-few": "{0} لتر",
        "unitPattern-count-many": "{0} لتر",
        "unitPattern-count-other": "{0} لتر"
      },
      "volume-milliliter": {
        "gender": "masculine",
        "unitPattern-count-zero": "{0} مليلتر",
        "unitPattern-count-one": "{0} مليلتر",
        "unitPattern-count-two": "{0} مليلتر",
        "unitPattern-count-few": "{0} مليلتر",
        "unitPattern-count-many": "{0} مليلتر",
        "unitPattern-count-other": "{0} مليلتر"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-other": "دولار أمريكي"
      },
      "EUR": {
        "displayName-count-other": "يورو"
      },
      "GBP": {
        "displayName-count-other": "جنيه إسترليني"
      },
      "PEN": {
        "displayName-count-other": "سول بيروفي"
      },
      "JPY": {
        "displayName-count-other": "ين ياباني"
      },
      "CNY": {
        "displayName-count-other": "يوان صيني"
      },
      "INR": {
        "displayName-count-other": "روبية هندي"
      },
      "KRW": {
        "displayName-count-other": "وون كوريا الجنوبية"
      },
      "RUB": {
        "displayName-count-other": "روبل روسي"
      },
      "BRL": {
        "displayName-count-other": "ريال برازيلي"
      },
      "TRY": {
        "displayName-count-other": "ليرة تركية"
      },
      "BDT": {
        "displayName-count-other": "تاكا بنغلاديشي"
      },
      "PKR": {
        "displayName-count-other": "روبية باكستاني"
      },
      "IDR": {
        "displayName-count-other": "روبية إندونيسية"
      },
      "VND": {
        "displayName-count-other": "دونج فيتنامي"
      }
    }
  },
  "fr": {
    "date_pattern": "d MMMM y",
    "months": [
      "janvier",
      "février",
      "mars",
      "avril",
      "mai",
      "juin",
      "juillet",
      "août",
      "septembre",
      "octobre",
      "novembre",
      "décembre"
    ],
    "units": {
      "concentr-percent": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} pour cent",
        "unitPattern-count-other": "{0} pour cent"
      },
      "duration-hour": {
        "gender": "feminine",
        "unitPattern-count-one": "{0} heure",
        "unitPattern-count-other": "{0} heures"
      },
      "duration-minute": {
        "gender": "feminine",
        "unitPattern-count-one": "{0} minute",
        "unitPattern-count-other": "{0} minutes"
      },
      "duration-second": {
        "gender": "feminine",
        "unitPattern-count-one": "{0} seconde",
        "unitPattern-count-other": "{0} secondes"
      },
      "length-meter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} mètre",
        "unitPattern-count-other": "{0} mètres"
      },
      "length-kilometer": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} kilomètre",
        "unitPattern-count-other": "{0} kilomètres"
      },
      "mass-gram": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} gramme",
        "unitPattern-count-other": "{0} grammes"
      },
      "mass-kilogram": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} kilogramme",
        "unitPattern-count-other": "{0} kilogrammes"
      },
      "temperature-celsius": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} degré Celsius",
        "unitPattern-count-other": "{0} degrés Celsius"
      },
      "volume-liter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} litre",
        "unitPattern-count-other": "{0} litres"
      },
      "volume-milliliter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} millilitre",
        "unitPattern-count-other": "{0} millilitres"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-one": "dollar des États-Unis",
        "displayName-count-other": "dollars des États-Unis"
      },
      "EUR": {
        "displayName-count-one": "euro",
        "displayName-count-other": "euros"
      },
      "GBP": {
        "displayName-count-one": "livre sterling",
        "displayName-count-other": "livres sterling"
      },
      "PEN": {
        "displayName-count-one": "sol péruvien",
        "displayName-count-other": "sols péruviens"
      },
      "JPY": {
        "displayName-count-one": "yen japonais",
        "displayName-count-other": "yens japonais"
      },
      "CNY": {
        "displayName-count-one": "yuan renminbi chinois",
        "displayName-count-other": "yuans renminbi chinois"
      },
      "INR": {
        "displayName-count-one": "roupie indienne",
        "displayName-count-other": "roupies indiennes"
      },
      "KRW": {
        "displayName-count-one": "won sud-coréen",
        "displayName-count-other": "wons sud-coréens"
      },
      "RUB": {
        "displayName-count-one": "rouble russe",
        "displayName-count-other": "roubles russes"
      },
      "BRL": {
        "displayName-count-one": "réal brésilien",
        "displayName-count-other": "réals brésiliens"
      },
      "TRY": {
        "displayName-count-one": "livre turque",
        "displayName-count-other": "livres turques"
      },
      "BDT": {
        "displayName-count-one": "taka bangladeshi",
        "displayName-count-other": "takas bangladeshis"
      },
      "PKR": {
        "displayName-count-one": "roupie pakistanaise",
        "displayName-count-other": "roupies pakistanaises"
      },
      "IDR": {
        "displayName-count-one": "roupie indonésienne",
        "displayName-count-other": "roupies indonésiennes"
      },
      "VND": {
        "displayName-count-one": "dông vietnamien",
        "displayName-count-other": "dôngs vietnamiens"
      }
    }
  },
  "pt": {
    "date_pattern": "d 'de' MMMM 'de' y",
    "months": [
      "janeiro",
      "fevereiro",
      "março",
      "abril",
      "maio",
      "junho",
      "julho",
      "agosto",
      "setembro",
      "outubro",
      "novembro",
      "dezembro"
    ],
    "units": {
      "concentr-percent": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} por cento",
        "unitPattern-count-other": "{0} por cento"
      },
      "duration-hour": {
        "gender": "feminine",
        "unitPattern-count-one": "{0} hora",
        "unitPattern-count-other": "{0} horas"
      },
      "duration-minute": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} minuto",
        "unitPattern-count-other": "{0} minutos"
      },
      "duration-second": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} segundo",
        "unitPattern-count-other": "{0} segundos"
      },
      "length-meter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} metro",
        "unitPattern-count-other": "{0} metros"
      },
      "length-kilometer": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} quilômetro",
        "unitPattern-count-other": "{0} quilômetros"
      },
      "mass-gram": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} grama",
        "unitPattern-count-other": "{0} gramas"
      },
      "mass-kilogram": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} quilograma",
        "unitPattern-count-other": "{0} quilogramas"
      },
      "temperature-celsius": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} grau Celsius",
        "unitPattern-count-other": "{0} graus Celsius"
      },
      "volume-liter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} litro",
        "unitPattern-count-other": "{0} litros"
      },
      "volume-milliliter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} mililitro",
        "unitPattern-count-other": "{0} mililitros"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-one": "Dólar americano",
        "displayName-count-other": "Dólares americanos"
      },
      "EUR": {
        "displayName-count-one": "Euro",
        "displayName-count-other": "Euros"
      },
      "GBP": {
        "displayName-count-one": "Libra esterlina",
        "displayName-count-other": "Libras esterlinas"
      },
      "PEN": {
        "displayName-count-one": "Novo sol peruano",
        "displayName-count-other": "Novos sóis peruanos"
      },
      "JPY": {
        "displayName-count-one": "Iene japonês",
        "displayName-count-other": "Ienes japoneses"
      },
      "CNY": {
        "displayName-count-one": "Yuan chinês",
        "displayName-count-other": "Yuans chineses"
      },
      "INR": {
        "displayName-count-one": "Rupia indiana",
        "displayName-count-other": "Rupias indianas"
      },
      "KRW": {
        "displayName-count-one": "Won sul-coreano",
        "displayName-count-other": "Wons sul-coreanos"
      },
      "RUB": {
        "displayName-count-one": "Rublo russo",
        "displayName-count-other": "Rublos russos"
      },
      "BRL": {
        "displayName-count-one": "Real brasileiro",
        "displayName-count-other": "Reais brasileiros"
      },
      "TRY": {
        "displayName-count-one": "Lira turca",
        "displayName-count-other": "Liras turcas"
      },
      "BDT": {
        "displayName-count-one": "Taka bengali",
        "displayName-count-other": "Takas bengalis"
      },
      "PKR": {
        "displayName-count-one": "Rupia paquistanesa",
        "displayName-count-other": "Rupias paquistanesas"
      },
      "IDR": {
        "displayName-count-one": "Rupia indonésia",
        "displayName-count-other": "Rupias indonésias"
      },
      "VND": {
        "displayName-count-one": "Dong vietnamita",
        "displayName-count-other": "Dongs vietnamitas"
      }
    }
  },
  "ru": {
    "date_pattern": "d MMMM y 'г'.",
    "months": [
      "января",
      "февраля",
      "марта",
      "апреля",
      "мая",
      "июня",
      "июля",
      "августа",
      "сентября",
      "октября",
      "ноября",
      "декабря"
    ],
    "units": {
      "concentr-percent": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} процент",
        "unitPattern-count-few": "{0} процента",
        "unitPattern-count-many": "{0} процентов",
        "unitPattern-count-other": "{0} процента"
      },
      "duration-hour": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} час",
        "unitPattern-count-few": "{0} часа",
        "unitPattern-count-many": "{0} часов",
        "unitPattern-count-other": "{0} часа"
      },
      "duration-minute": {
        "gender": "feminine",
        "unitPattern-count-one": "{0} минута",
        "unitPattern-count-few": "{0} минуты",
        "unitPattern-count-many": "{0} минут",
        "unitPattern-count-other": "{0} минуты"
      },
      "duration-second": {
        "gender": "feminine",
        "unitPattern-count-one": "{0} секунда",
        "unitPattern-count-few": "{0} секунды",
        "unitPattern-count-many": "{0} секунд",
        "unitPattern-count-other": "{0} секунды"
      },
      "length-meter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} метр",
        "unitPattern-count-few": "{0} метра",
        "unitPattern-count-many": "{0} метров",
        "unitPattern-count-other": "{0} метра"
      },
      "length-kilometer": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} километр",
        "unitPattern-count-few": "{0} километра",
        "unitPattern-count-many": "{0} километров",
        "unitPattern-count-other": "{0} километра"
      },
      "mass-gram": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} грамм",
        "unitPattern-count-few": "{0} грамма",
        "unitPattern-count-many": "{0} грамм",
        "unitPattern-count-other": "{0} грамма"
      },
      "mass-kilogram": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} килограмм",
        "unitPattern-count-few": "{0} килограмма",
        "unitPattern-count-many": "{0} килограмм",
        "unitPattern-count-other": "{0} килограмма"
      },
      "temperature-celsius": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} градус Цельсия",
        "unitPattern-count-few": "{0} градуса Цельсия",
        "unitPattern-count-many": "{0} градусов Цельсия",
        "unitPattern-count-other": "{0} градуса Цельсия"
      },
      "volume-liter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} литр",
        "unitPattern-count-few": "{0} литра",
        "unitPattern-count-many": "{0} литров",
        "unitPattern-count-other": "{0} литра"
      },
      "volume-milliliter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} миллилитр",
        "unitPattern-count-few": "{0} миллилитра",
        "unitPattern-count-many": "{0} миллилитров",
        "unitPattern-count-other": "{0} миллилитра"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-one": "доллар США",
        "displayName-count-few": "доллара США",
        "displayName-count-many": "долларов США",
        "displayName-count-other": "доллара США"
      },
      "EUR": {
        "displayName-count-other": "евро"
      },
      "GBP": {
        "displayName-count-one": "британский фунт стерлингов",
        "displayName-count-few": "британских фунта стерлингов",
        "displayName-count-many": "британских фунтов стерлингов",
        "displayName-count-other": "британского фунта стерлингов"
      },
      "PEN": {
        "displayName-count-one": "перуанский соль",
        "displayName-count-few": "перуанских соля",
        "displayName-count-many": "перуанских солей",
        "displayName-count-other": "перуанского соля"
      },
      "JPY": {
        "displayName-count-one": "японская иена",
        "displayName-count-few": "японские иены",
        "displayName-count-many": "японских иен",
        "displayName-count-other": "японской иены"
      },
      "CNY": {
        "displayName-count-one": "китайский юань",
        "displayName-count-few": "китайских юаня",
        "displayName-count-many": "китайских юаней",
        "displayName-count-other": "китайского юаня"
      },
      "INR": {
        "displayName-count-one": "индийская рупия",
        "displayName-count-few": "индийские рупии",
        "displayName-count-many": "индийских рупий",
        "displayName-count-other": "индийской рупии"
      },
      "KRW": {
        "displayName-count-one": "южнокорейская вона",
        "displayName-count-few": "южнокорейские воны",
        "displayName-count-many": "южнокорейских вон",
        "displayName-count-other": "южнокорейской воны"
      },
      "RUB": {
        "displayName-count-one": "российский рубль",
        "displayName-count-few": "российских рубля",
        "displayName-count-many": "российских рублей",
        "displayName-count-other": "российского рубля"
      },
      "BRL": {
        "displayName-count-one": "бразильский реал",
        "displayName-count-few": "бразильских реала",
        "displayName-count-many": "бразильских реалов",
        "displayName-count-other": "бразильского реала"
      },
      "TRY": {
        "displayName-count-one": "турецкая лира",
        "displayName-count-few": "турецкие лиры",
        "displayName-count-many": "турецких лир",
        "displayName-count-other": "турецкой лиры"
      },
      "BDT": {
        "displayName-count-one": "бангладешская така",
        "displayName-count-few": "бангладешские таки",
        "displayName-count-many": "бангладешских так",
        "displayName-count-other": "бангладешской таки"
      },
      "PKR": {
        "displayName-count-one": "пакистанская рупия",
        "displayName-count-few": "пакистанские рупии",
        "displayName-count-many": "пакистанских рупий",
        "displayName-count-other": "пакистанской рупии"
      },
      "IDR": {
        "displayName-count-one": "индонезийская рупия",
        "displayName-count-few": "индонезийские рупии",
        "displayName-count-many": "индонезийских рупий",
        "displayName-count-other": "индонезийской рупии"
      },
      "VND": {
        "displayName-count-one": "вьетнамский донг",
        "displayName-count-few": "вьетнамских донга",
        "displayName-count-many": "вьетнамских донгов",
        "displayName-count-other": "вьетнамского донга"
      }
    }
  },
  "bn": {
    "date_pattern": "d MMMM, y",
    "months": [
      "জানুয়ারি",
      "ফেব্রুয়ারি",
      "মার্চ",
      "এপ্রিল",
      "মে",
      "জুন",
      "জুলাই",
      "আগস্ট",
      "সেপ্টেম্বর",
      "অক্টোবর",
      "নভেম্বর",
      "ডিসেম্বর"
    ],
    "units": {
      "concentr-percent": {
        "unitPattern-count-one": "{0}শতাংশ",
        "unitPattern-count-other": "{0}শতাংশ"
      },
      "duration-hour": {
        "unitPattern-count-one": "{0} ঘণ্টা",
        "unitPattern-count-other": "{0} ঘণ্টা"
      },
      "duration-minute": {
        "unitPattern-count-one": "{0} মিনিট",
        "unitPattern-count-other": "{0} মিনিট"
      },
      "duration-second": {
        "unitPattern-count-one": "{0} সেকেন্ড",
        "unitPattern-count-other": "{0} সেকেন্ড"
      },
      "length-meter": {
        "unitPattern-count-one": "{0} মিটার",
        "unitPattern-count-other": "{0} মিটার"
      },
      "length-kilometer": {
        "unitPattern-count-one": "{0} কিলোমিটার",
        "unitPattern-count-other": "{0} কিলোমিটার"
      },
      "mass-gram": {
        "unitPattern-count-one": "{0} গ্রাম",
        "unitPattern-count-other": "{0} গ্রাম"
      },
      "mass-kilogram": {
        "unitPattern-count-one": "{0} কিলোগ্রাম",
        "unitPattern-count-other": "{0} কিলোগ্রাম"
      },
      "temperature-celsius": {
        "unitPattern-count-one": "{0} ডিগ্রী সেলসিয়াস",
        "unitPattern-count-other": "{0} ডিগ্রী সেলসিয়াস"
      },
      "volume-liter": {
        "unitPattern-count-one": "{0} লিটার",
        "unitPattern-count-other": "{0} লিটার"
      },
      "volume-milliliter": {
        "unitPattern-count-one": "{0} মিলিলিটার",
        "unitPattern-count-other": "{0} মিলিলিটার"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-other": "মার্কিন ডলার"
      },
      "EUR": {
        "displayName-count-other": "ইউরো"
      },
      "GBP": {
        "displayName-count-other": "ব্রিটিশ পাউন্ড"
      },
      "PEN": {
        "displayName-count-other": "পেরুভিয়ান সোল"
      },
      "JPY": {
        "displayName-count-other": "জাপানি ইয়েন"
      },
      "CNY": {
        "displayName-count-one": "চীনা ইউয়ান",
        "displayName-count-other": "চীনা ইউয়ান"
      },
      "INR": {
        "displayName-count-one": "ভারতীয় টাকা",
        "displayName-count-other": "ভারতীয় টাকা"
      },
      "KRW": {
        "displayName-count-other": "দক্ষিণ কোরিয়ান ওন"
      },
      "RUB": {
        "displayName-count-other": "রাশিয়ান রুবেল"
      },
      "BRL": {
        "displayName-count-other": "ব্রাজিলিয়ান রিয়েল"
      },
      "TRY": {
        "displayName-count-other": "তুর্কি লিরা"
      },
      "BDT": {
        "displayName-count-other": "বাংলাদেশী টাকা"
      },
      "PKR": {
        "displayName-count-other": "পাকিস্তানি রুপি"
      },
      "IDR": {
        "displayName-count-other": "ইন্দোনেশিয়ান রুপিয়াহ"
      },
      "VND": {
        "displayName-count-one": "ভিয়েতনামী ডং",
        "displayName-count-other": "ভিয়েতনামী ডং"
      }
    }
  },
  "ur": {
    "date_pattern": "d MMMM، y",
    "months": [
      "جنوری",
      "فروری",
      "مارچ",
      "اپریل",
      "مئی",
      "جون",
      "جولائی",
      "اگست",
      "ستمبر",
      "اکتوبر",
      "نومبر",
      "دسمبر"
    ],
    "units": {
      "concentr-percent": {
        "gender": "feminine",
        "unitPattern-count-one": "{0} فیصد",
        "unitPattern-count-other": "{0} فیصد"
      },
      "duration-hour": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} گھنٹہ",
        "unitPattern-count-other": "{0} گھنٹے"
      },
      "duration-minute": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} منٹ",
        "unitPattern-count-other": "{0} منٹ"
      },
      "duration-second": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} سیکنڈ",
        "unitPattern-count-other": "{0} سیکنڈ"
      },
      "length-meter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} میٹر",
        "unitPattern-count-other": "{0} میٹر"
      },
      "length-kilometer": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} کلو میٹر",
        "unitPattern-count-other": "{0} کلو میٹر"
      },
      "mass-gram": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} گرام",
        "unitPattern-count-other": "{0} گرام"
      },
      "mass-kilogram": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} کلو گرام",
        "unitPattern-count-other": "{0} کلو گرام"
      },
      "temperature-celsius": {
        "gender": "feminine",
        "unitPattern-count-one": "{0} ڈگری سیلسیس",
        "unitPattern-count-other": "{0} ڈگری سیلسیس"
      },
      "volume-liter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} لیٹر",
        "unitPattern-count-other": "{0} لیٹر"
      },
      "volume-milliliter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} ملی لیٹر",
        "unitPattern-count-other": "{0} ملی لیٹر"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-other": "امریکی ڈالر"
      },
      "EUR": {
        "displayName-count-other": "یورو"
      },
      "GBP": {
        "displayName-count-other": "برطانوی پاؤنڈ"
      },
      "PEN": {
        "displayName-count-other": "پیروویئن سول"
      },
      "JPY": {
        "displayName-count-other": "جاپانی ین"
      },
      "CNY": {
        "displayName-count-other": "چینی یوآن"
      },
      "INR": {
        "displayName-count-one": "بھارتی روپیہ",
        "displayName-count-other": "بھارتی روپے"
      },
      "KRW": {
        "displayName-count-other": "جنوبی کوریائی وون"
      },
      "RUB": {
        "displayName-count-other": "روسی روبل"
      },
      "BRL": {
        "displayName-count-other": "برازیلی ریئل"
      },
      "TRY": {
        "displayName-count-other": "ترکی لیرا"
      },
      "BDT": {
        "displayName-count-other": "بنگلہ دیشی ٹکا"
      },
      "PKR": {
        "displayName-count-other": "پاکستانی روپیہ"
      },
      "IDR": {
        "displayName-count-other": "انڈونیشین روپیہ"
      },
      "VND": {
        "displayName-count-other": "ویتنامی ڈانگ"
      }
    }
  },
  "id": {
    "date_pattern": "d MMMM y",
    "months": [
      "Januari",
      "Februari",
      "Maret",
      "April",
      "Mei",
      "Juni",
      "Juli",
      "Agustus",
      "September",
      "Oktober",
      "November",
      "Desember"
    ],
    "units": {
      "concentr-percent": {
        "unitPattern-count-other": "{0} persen"
      },
      "duration-hour": {
        "unitPattern-count-other": "{0} jam"
      },
      "duration-minute": {
        "unitPattern-count-other": "{0} menit"
      },
      "duration-second": {
        "unitPattern-count-other": "{0} detik"
      },
      "length-meter": {
        "unitPattern-count-other": "{0} meter"
      },
      "length-kilometer": {
        "unitPattern-count-other": "{0} kilometer"
      },
      "mass-gram": {
        "unitPattern-count-other": "{0} gram"
      },
      "mass-kilogram": {
        "unitPattern-count-other": "{0} kilogram"
      },
      "temperature-celsius": {
        "unitPattern-count-other": "{0} derajat Celsius"
      },
      "volume-liter": {
        "unitPattern-count-other": "{0} liter"
      },
      "volume-milliliter": {
        "unitPattern-count-other": "{0} mililiter"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-other": "Dolar Amerika Serikat"
      },
      "EUR": {
        "displayName-count-other": "Euro"
      },
      "GBP": {
        "displayName-count-other": "Pound Inggris"
      },
      "PEN": {
        "displayName-count-other": "Sol Peru"
      },
      "JPY": {
        "displayName-count-other": "Yen Jepang"
      },
      "CNY": {
        "displayName-count-other": "Yuan Tiongkok"
      },
      "INR": {
        "displayName-count-other": "Rupee India"
      },
      "KRW": {
        "displayName-count-other": "Won Korea Selatan"
      },
      "RUB": {
        "displayName-count-other": "Rubel Rusia"
      },
      "BRL": {
        "displayName-count-other": "Real Brasil"
      },
      "TRY": {
        "displayName-count-other": "Lira Turki"
      },
      "BDT": {
        "displayName-count-other": "Taka Bangladesh"
      },
      "PKR": {
        "displayName-count-other": "Rupee Pakistan"
      },
      "IDR": {
        "displayName-count-other": "Rupiah Indonesia"
      },
      "VND": {
        "displayName-count-other": "Dong Vietnam"
      }
    }
  },
  "de": {
    "date_pattern": "d. MMMM y",
    "months": [
      "Januar",
      "Februar",
      "März",
      "April",
      "Mai",
      "Juni",
      "Juli",
      "August",
      "September",
      "Oktober",
      "November",
      "Dezember"
    ],
    "units": {
      "concentr-percent": {
        "gender": "neuter",
        "unitPattern-count-one": "{0} Prozent",
        "unitPattern-count-other": "{0} Prozent"
      },
      "duration-hour": {
        "gender": "feminine",
        "unitPattern-count-one": "{0} Stunde",
        "unitPattern-count-other": "{0} Stunden"
      },
      "duration-minute": {
        "gender": "feminine",
        "unitPattern-count-one": "{0} Minute",
        "unitPattern-count-other": "{0} Minuten"
      },
      "duration-second": {
        "gender": "feminine",
        "unitPattern-count-one": "{0} Sekunde",
        "unitPattern-count-other": "{0} Sekunden"
      },
      "length-meter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} Meter",
        "unitPattern-count-other": "{0} Meter"
      },
      "length-kilometer": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} Kilometer",
        "unitPattern-count-other": "{0} Kilometer"
      },
      "mass-gram": {
        "gender": "neuter",
        "unitPattern-count-one": "{0} Gramm",
        "unitPattern-count-other": "{0} Gramm"
      },
      "mass-kilogram": {
        "gender": "neuter",
        "unitPattern-count-one": "{0} Kilogramm",
        "unitPattern-count-other": "{0} Kilogramm"
      },
      "temperature-celsius": {
        "gender": "neuter",
        "unitPattern-count-one": "{0} Grad Celsius",
        "unitPattern-count-other": "{0} Grad Celsius"
      },
      "volume-liter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} Liter",
        "unitPattern-count-other": "{0} Liter"
      },
      "volume-milliliter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} Milliliter",
        "unitPattern-count-other": "{0} Milliliter"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-other": "US-Dollar"
      },
      "EUR": {
        "displayName-count-other": "Euro"
      },
      "GBP": {
        "displayName-count-one": "Britisches Pfund",
        "displayName-count-other": "Britische Pfund"
      },
      "PEN": {
        "displayName-count-one": "Peruanischer Sol",
        "displayName-count-other": "Peruanische Sol"
      },
      "JPY": {
        "displayName-count-one": "Japanischer Yen",
        "displayName-count-other": "Japanische Yen"
      },
      "CNY": {
        "displayName-count-one": "Chinesischer Yuan",
        "displayName-count-other": "Renminbi Yuan"
      },
      "INR": {
        "displayName-count-one": "Indische Rupie",
        "displayName-count-other": "Indische Rupien"
      },
      "KRW": {
        "displayName-count-one": "Südkoreanischer Won",
        "displayName-count-other": "Südkoreanische Won"
      },
      "RUB": {
        "displayName-count-one": "Russischer Rubel",
        "displayName-count-other": "Russische Rubel"
      },
      "BRL": {
        "displayName-count-one": "Brasilianischer Real",
        "displayName-count-other": "Brasilianische Real"
      },
      "TRY": {
        "displayName-count-other": "Türkische Lira"
      },
      "BDT": {
        "displayName-count-other": "Bangladesch-Taka"
      },
      "PKR": {
        "displayName-count-one": "Pakistanische Rupie",
        "displayName-count-other": "Pakistanische Rupien"
      },
      "IDR": {
        "displayName-count-other": "Indonesische Rupiah"
      },
      "VND": {
        "displayName-count-one": "Vietnamesischer Dong",
        "displayName-count-other": "Vietnamesische Dong"
      }
    }
  },
  "ja": {
    "date_pattern": "y年M月d日",
    "months": [
      "1月",
      "2月",
      "3月",
      "4月",
      "5月",
      "6月",
      "7月",
      "8月",
      "9月",
      "10月",
      "11月",
      "12月"
    ],
    "units": {
      "concentr-percent": {
        "unitPattern-count-other": "{0} パーセント"
      },
      "duration-hour": {
        "unitPattern-count-other": "{0} 時間"
      },
      "duration-minute": {
        "unitPattern-count-other": "{0} 分"
      },
      "duration-second": {
        "unitPattern-count-other": "{0} 秒"
      },
      "length-meter": {
        "unitPattern-count-other": "{0} メートル"
      },
      "length-kilometer": {
        "unitPattern-count-other": "{0} キロメートル"
      },
      "mass-gram": {
        "unitPattern-count-other": "{0} グラム"
      },
      "mass-kilogram": {
        "unitPattern-count-other": "{0} キログラム"
      },
      "temperature-celsius": {
        "unitPattern-count-other": "摂氏 {0} 度"
      },
      "volume-liter": {
        "unitPattern-count-other": "{0} リットル"
      },
      "volume-milliliter": {
        "unitPattern-count-other": "{0} ミリリットル"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-other": "米ドル"
      },
      "EUR": {
        "displayName-count-other": "ユーロ"
      },
      "GBP": {
        "displayName-count-other": "英国ポンド"
      },
      "PEN": {
        "displayName-count-other": "ペルー ソル"
      },
      "JPY": {
        "displayName-count-other": "円"
      },
      "CNY": {
        "displayName-count-other": "中国人民元"
      },
      "INR": {
        "displayName-count-other": "インド ルピー"
      },
      "KRW": {
        "displayName-count-other": "韓国ウォン"
      },
      "RUB": {
        "displayName-count-other": "ロシア ルーブル"
      },
      "BRL": {
        "displayName-count-other": "ブラジル レアル"
      },
      "TRY": {
        "displayName-count-other": "トルコ リラ"
      },
      "BDT": {
        "displayName-count-other": "バングラデシュ タカ"
      },
      "PKR": {
        "displayName-count-other": "パキスタン ルピー"
      },
      "IDR": {
        "displayName-count-other": "インドネシア ルピア"
      },
      "VND": {
        "displayName-count-other": "ベトナム ドン"
      }
    }
  },
  "pa": {
    "date_pattern": "d MMMM y",
    "months": [
      "ਜਨਵਰੀ",
      "ਫ਼ਰਵਰੀ",
      "ਮਾਰਚ",
      "ਅਪ੍ਰੈਲ",
      "ਮਈ",
      "ਜੂਨ",
      "ਜੁਲਾਈ",
      "ਅਗਸਤ",
      "ਸਤੰਬਰ",
      "ਅਕਤੂਬਰ",
      "ਨਵੰਬਰ",
      "ਦਸੰਬਰ"
    ],
    "units": {
      "concentr-percent": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} ਪ੍ਰਤਿਸ਼ਤ",
        "unitPattern-count-other": "{0} ਪ੍ਰਤਿਸ਼ਤ"
      },
      "duration-hour": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} ਘੰਟਾ",
        "unitPattern-count-other": "{0} ਘੰਟੇ"
      },
      "duration-minute": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} ਮਿੰਟ",
        "unitPattern-count-other": "{0} ਮਿੰਟ"
      },
      "duration-second": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} ਸਕਿੰਟ",
        "unitPattern-count-other": "{0} ਸਕਿੰਟ"
      },
      "length-meter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} ਮੀਟਰ",
        "unitPattern-count-other": "{0} ਮੀਟਰ"
      },
      "length-kilometer": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} ਕਿਲੋਮੀਟਰ",
        "unitPattern-count-other": "{0} ਕਿਲੋਮੀਟਰ"
      },
      "mass-gram": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} ਗ੍ਰਾਮ",
        "unitPattern-count-other": "{0} ਗ੍ਰਾਮ"
      },
      "mass-kilogram": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} ਕਿਲੋਗ੍ਰਾਮ",
        "unitPattern-count-other": "{0} ਕਿਲੋਗ੍ਰਾਮ"
      },
      "temperature-celsius": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} ਡਿਗਰੀ ਸੈਲਸੀਅਸ",
        "unitPattern-count-other": "{0} ਡਿਗਰੀ ਸੈਲਸੀਅਸ"
      },
      "volume-liter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} ਲਿਟਰ",
        "unitPattern-count-other": "{0} ਲਿਟਰ"
      },
      "volume-milliliter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} ਮਿਲੀਲਿਟਰ",
        "unitPattern-count-other": "{0} ਮਿਲੀਲਿਟਰ"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-other": "ਯੂ.ਐਸ. ਡਾਲਰ"
      },
      "EUR": {
        "displayName-count-other": "ਯੂਰੋ"
      },
      "GBP": {
        "displayName-count-other": "ਬ੍ਰਿਟਿਸ਼ ਪੌਂਡ"
      },
      "PEN": {
        "displayName-count-other": "ਪੇਰੂਵੀਅਨ ਸੋਲ"
      },
      "JPY": {
        "displayName-count-other": "ਜਪਾਨੀ ਯੇਨ"
      },
      "CNY": {
        "displayName-count-other": "ਚੀਨੀ ਯੁਆਨ"
      },
      "INR": {
        "displayName-count-one": "ਭਾਰਤੀ ਰੁਪਇਆ",
        "displayName-count-other": "ਭਾਰਤੀ ਰੁਪਏ"
      },
      "KRW": {
        "displayName-count-other": "ਦੱਖਣੀ ਕੋਰੀਆਈ ਵੋਨ"
      },
      "RUB": {
        "displayName-count-other": "ਰੂਸੀ ਰੂਬਲ"
      },
      "BRL": {
        "displayName-count-other": "ਬ੍ਰਾਜ਼ੀਲੀਆਈ ਰੀਅਲ"
      },
      "TRY": {
        "displayName-count-other": "ਤੁਰਕੀ ਲੀਰਾ"
      },
      "BDT": {
        "displayName-count-other": "ਬੰਗਲਾਦੇਸ਼ੀ ਟਕਾ"
      },
      "PKR": {
        "displayName-count-one": "ਪਾਕਿਸਤਾਨੀ ਰੁਪਇਆ",
        "displayName-count-other": "ਪਾਕਿਸਤਾਨੀ ਰੁਪਏ"
      },
      "IDR": {
        "displayName-count-other": "ਇੰਡੋਨੇਸ਼ੀਆਈ ਰੁਪਿਆਹ"
      },
      "VND": {
        "displayName-count-other": "ਵੀਅਤਨਾਮੀ ਡੋਂਗ"
      }
    }
  },
  "mr": {
    "date_pattern": "d MMMM, y",
    "months": [
      "जानेवारी",
      "फेब्रुवारी",
      "मार्च",
      "एप्रिल",
      "मे",
      "जून",
      "जुलै",
      "ऑगस्ट",
      "सप्टेंबर",
      "ऑक्टोबर",
      "नोव्हेंबर",
      "डिसेंबर"
    ],
    "units": {
      "concentr-percent": {
        "gender": "neuter",
        "unitPattern-count-one": "{0} टक्के",
        "unitPattern-count-other": "{0} टक्के"
      },
      "duration-hour": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} तास",
        "unitPattern-count-other": "{0} तास"
      },
      "duration-minute": {
        "gender": "neuter",
        "unitPattern-count-one": "{0} मिनिट",
        "unitPattern-count-other": "{0} मिनिटे"
      },
      "duration-second": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} सेकंद",
        "unitPattern-count-other": "{0} सेकंद"
      },
      "length-meter": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} मीटर",
        "unitPattern-count-other": "{0} मीटर"
      },
      "length-kilometer": {
        "gender": "neuter",
        "unitPattern-count-one": "{0} किलोमीटर",
        "unitPattern-count-other": "{0} किलोमीटर"
      },
      "mass-gram": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} ग्रॅम",
        "unitPattern-count-other": "{0} ग्रॅम"
      },
      "mass-kilogram": {
        "gender": "masculine",
        "unitPattern-count-one": "{0} किलोग्रॅम",
        "unitPattern-count-other": "{0} किलोग्रॅम"
      },
      "temperature-celsius": {
        "gender": "neuter",
        "unitPattern-count-one": "{0} अंश सेल्सिअस",
        "unitPattern-count-other": "{0} अंश सेल्सिअस"
      },
      "volume-liter": {
        "gender": "neuter",
        "unitPattern-count-one": "{0} लीटर",
        "unitPattern-count-other": "{0} लीटर"
      },
      "volume-milliliter": {
        "gender": "neuter",
        "unitPattern-count-one": "{0} मिलीलीटर",
        "unitPattern-count-other": "{0} मिलीलीटर"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-one": "यूएस डॉलर",
        "displayName-count-other": "यूएस डॉलर्स"
      },
      "EUR": {
        "displayName-count-other": "युरो"
      },
      "GBP": {
        "displayName-count-one": "ब्रिटिश पाऊंड",
        "displayName-count-other": "ब्रिटिश पाऊंड्स"
      },
      "PEN": {
        "displayName-count-one": "पेरुवियन सोल",
        "displayName-count-other": "पेरुवियन सोल्स"
      },
      "JPY": {
        "displayName-count-other": "जपानी येन"
      },
      "CNY": {
        "displayName-count-other": "चीनी युआन"
      },
      "INR": {
        "displayName-count-one": "भारतीय रुपया",
        "displayName-count-other": "भारतीय रुपये"
      },
      "KRW": {
        "displayName-count-other": "दक्षिण कोरियन वॉन"
      },
      "RUB": {
        "displayName-count-one": "रशियन रुबल",
        "displayName-count-other": "रशियन रुबल्स"
      },
      "BRL": {
        "displayName-count-one": "ब्राझिलियन रियाल",
        "displayName-count-other": "ब्राझिलियन रियाल्स"
      },
      "TRY": {
        "displayName-count-other": "तुर्की लिरा"
      },
      "BDT": {
        "displayName-count-other": "बांगलादेशी टका"
      },
      "PKR": {
        "displayName-count-one": "पाकिस्तानी रुपया",
        "displayName-count-other": "पाकिस्तानी रुपये"
      },
      "IDR": {
        "displayName-count-other": "इंडोनेशियन रुपिया"
      },
      "VND": {
        "displayName-count-other": "व्हिएतनामी डोंग"
      }
    }
  },
  "te": {
    "date_pattern": "d MMMM, y",
    "months": [
      "జనవరి",
      "ఫిబ్రవరి",
      "మార్చి",
      "ఏప్రిల్",
      "మే",
      "జూన్",
      "జులై",
      "ఆగస్టు",
      "సెప్టెంబర్",
      "అక్టోబర్",
      "నవంబర్",
      "డిసెంబర్"
    ],
    "units": {
      "concentr-percent": {
        "unitPattern-count-one": "{0} శాతం",
        "unitPattern-count-other": "{0} శాతం"
      },
      "duration-hour": {
        "unitPattern-count-one": "{0} గంట",
        "unitPattern-count-other": "{0} గంటలు"
      },
      "duration-minute": {
        "unitPattern-count-one": "{0} నిమిషం",
        "unitPattern-count-other": "{0} నిమిషాలు"
      },
      "duration-second": {
        "unitPattern-count-one": "{0} సెకను",
        "unitPattern-count-other": "{0} సెకన్లు"
      },
      "length-meter": {
        "unitPattern-count-one": "{0} మీటరు",
        "unitPattern-count-other": "{0} మీటర్లు"
      },
      "length-kilometer": {
        "unitPattern-count-one": "{0} కిలోమీటరు",
        "unitPattern-count-other": "{0} కిలోమీటర్లు"
      },
      "mass-gram": {
        "unitPattern-count-one": "{0} గ్రాము",
        "unitPattern-count-other": "{0} గ్రాములు"
      },
      "mass-kilogram": {
        "unitPattern-count-one": "{0} కిలోగ్రాము",
        "unitPattern-count-other": "{0} కిలోగ్రాములు"
      },
      "temperature-celsius": {
        "unitPattern-count-one": "{0} డిగ్రీ సెల్సియస్",
        "unitPattern-count-other": "{0} డిగ్రీల సెల్సియస్"
      },
      "volume-liter": {
        "unitPattern-count-one": "{0} లీటరు",
        "unitPattern-count-other": "{0} లీటర్లు"
      },
      "volume-milliliter": {
        "unitPattern-count-one": "{0} మిల్లీలీటరు",
        "unitPattern-count-other": "{0} మిల్లీలీటర్లు"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-one": "అమెరికా డాలర్",
        "displayName-count-other": "అమెరికా డాలర్‌లు"
      },
      "EUR": {
        "displayName-count-one": "యూరో",
        "displayName-count-other": "యూరోలు"
      },
      "GBP": {
        "displayName-count-one": "బ్రిటిష్ పౌండ్",
        "displayName-count-other": "బ్రిటిష్ పౌండ్‌లు"
      },
      "PEN": {
        "displayName-count-one": "పెరువియన్ సోల్",
        "displayName-count-other": "పెరువియన్ సోల్‌లు"
      },
      "JPY": {
        "displayName-count-other": "జపనీస్ యెన్"
      },
      "CNY": {
        "displayName-count-other": "చైనీస్ యువాన్"
      },
      "INR": {
        "displayName-count-one": "భారతదేశ రూపాయి",
        "displayName-count-other": "భారతదేశ రూపాయలు"
      },
      "KRW": {
        "displayName-count-other": "దక్షిణ కొరియా వోన్"
      },
      "RUB": {
        "displayName-count-one": "రష్యన్ రూబల్",
        "displayName-count-other": "రష్యన్ రూబల్‌లు"
      },
      "BRL": {
        "displayName-count-one": "బ్రెజిలియన్ రియల్",
        "displayName-count-other": "బ్రెజిలియన్ రియల్‌లు"
      },
      "TRY": {
        "displayName-count-other": "టర్కిష్ లీరా"
      },
      "BDT": {
        "displayName-count-one": "బాంగ్లాదేశ్ టాకా",
        "displayName-count-other": "బాంగ్లాదేశ్ టాకాలు"
      },
      "PKR": {
        "displayName-count-one": "పాకిస్థాన్ రూపాయి",
        "displayName-count-other": "పాకిస్థాన్ రూపాయలు"
      },
      "IDR": {
        "displayName-count-one": "ఇండోనేషియా రూపాయి",
        "displayName-count-other": "ఇండోనేషియా రూపాయలు"
      },
      "VND": {
        "displayName-count-other": "వియత్నామీయుల డాంగ్"
      }
    }
  },
  "tr": {
    "date_pattern": "d MMMM y",
    "months": [
      "Ocak",
      "Şubat",
      "Mart",
      "Nisan",
      "Mayıs",
      "Haziran",
      "Temmuz",
      "Ağustos",
      "Eylül",
      "Ekim",
      "Kasım",
      "Aralık"
    ],
    "units": {
      "concentr-percent": {
        "unitPattern-count-one": "yüzde {0}",
        "unitPattern-count-other": "yüzde {0}"
      },
      "duration-hour": {
        "unitPattern-count-one": "{0} saat",
        "unitPattern-count-other": "{0} saat"
      },
      "duration-minute": {
        "unitPattern-count-one": "{0} dakika",
        "unitPattern-count-other": "{0} dakika"
      },
      "duration-second": {
        "unitPattern-count-one": "{0} saniye",
        "unitPattern-count-other": "{0} saniye"
      },
      "length-meter": {
        "unitPattern-count-one": "{0} metre",
        "unitPattern-count-other": "{0} metre"
      },
      "length-kilometer": {
        "unitPattern-count-one": "{0} kilometre",
        "unitPattern-count-other": "{0} kilometre"
      },
      "mass-gram": {
        "unitPattern-count-one": "{0} gram",
        "unitPattern-count-other": "{0} gram"
      },
      "mass-kilogram": {
        "unitPattern-count-one": "{0} kilogram",
        "unitPattern-count-other": "{0} kilogram"
      },
      "temperature-celsius": {
        "unitPattern-count-one": "{0} santigrat derece",
        "unitPattern-count-other": "{0} santigrat derece"
      },
      "volume-liter": {
        "unitPattern-count-one": "{0} litre",
        "unitPattern-count-other": "{0} litre"
      },
      "volume-milliliter": {
        "unitPattern-count-one": "{0} mililitre",
        "unitPattern-count-other": "{0} mililitre"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-other": "ABD doları"
      },
      "EUR": {
        "displayName-count-other": "Euro"
      },
      "GBP": {
        "displayName-count-other": "İngiliz sterlini"
      },
      "PEN": {
        "displayName-count-other": "Peru solü"
      },
      "JPY": {
        "displayName-count-other": "Japon yeni"
      },
      "CNY": {
        "displayName-count-other": "Çin yuanı"
      },
      "INR": {
        "displayName-count-other": "Hindistan rupisi"
      },
      "KRW": {
        "displayName-count-other": "Güney Kore wonu"
      },
      "RUB": {
        "displayName-count-other": "Rus rublesi"
      },
      "BRL": {
        "displayName-count-other": "Brezilya reali"
      },
      "TRY": {
        "displayName-count-other": "Türk lirası"
      },
      "BDT": {
        "displayName-count-other": "Bangladeş takası"
      },
      "PKR": {
        "displayName-count-other": "Pakistan rupisi"
      },
      "IDR": {
        "displayName-count-other": "Endonezya rupisi"
      },
      "VND": {
        "displayName-count-other": "Vietnam dongu"
      }
    }
  },
  "ta": {
    "date_pattern": "d MMMM, y",
    "months": [
      "ஜனவரி",
      "பிப்ரவரி",
      "மார்ச்",
      "ஏப்ரல்",
      "மே",
      "ஜூன்",
      "ஜூலை",
      "ஆகஸ்ட்",
      "செப்டம்பர்",
      "அக்டோபர்",
      "நவம்பர்",
      "டிசம்பர்"
    ],
    "units": {
      "concentr-percent": {
        "unitPattern-count-one": "{0} சதவீதம்",
        "unitPattern-count-other": "{0} சதவீதம்"
      },
      "duration-hour": {
        "unitPattern-count-one": "{0} மணிநேரம்",
        "unitPattern-count-other": "{0} மணிநேரம்"
      },
      "duration-minute": {
        "unitPattern-count-one": "{0} நிமிடம்",
        "unitPattern-count-other": "{0} நிமிடங்கள்"
      },
      "duration-second": {
        "unitPattern-count-one": "{0} விநாடி",
        "unitPattern-count-other": "{0} விநாடிகள்"
      },
      "length-meter": {
        "unitPattern-count-one": "{0} மீட்டர்",
        "unitPattern-count-other": "{0} மீட்டர்கள்"
      },
      "length-kilometer": {
        "unitPattern-count-one": "{0} கிலோமீட்டர்",
        "unitPattern-count-other": "{0} கிலோமீட்டர்கள்"
      },
      "mass-gram": {
        "unitPattern-count-one": "{0} கிராம்",
        "unitPattern-count-other": "{0} கிராம்கள்"
      },
      "mass-kilogram": {
        "unitPattern-count-one": "{0} கிலோகிராம்",
        "unitPattern-count-other": "{0} கிலோகிராம்கள்"
      },
      "temperature-celsius": {
        "unitPattern-count-one": "{0} டிகிரி செல்சியஸ்",
        "unitPattern-count-other": "{0} டிகிரி செல்சியஸ்"
      },
      "volume-liter": {
        "unitPattern-count-one": "{0} லிட்டர்",
        "unitPattern-count-other": "{0} லிட்டர்கள்"
      },
      "volume-milliliter": {
        "unitPattern-count-one": "{0} மில்லிலிட்டர்",
        "unitPattern-count-other": "{0} மில்லிலிட்டர்கள்"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-one": "அமெரிக்க டாலர்",
        "displayName-count-other": "அமெரிக்க டாலர்கள்"
      },
      "EUR": {
        "displayName-count-one": "யூரோ",
        "displayName-count-other": "யூரோக்கள்"
      },
      "GBP": {
        "displayName-count-one": "பிரிட்டிஷ் பவுண்டு",
        "displayName-count-other": "பிரிட்டிஷ் பவுண்டுகள்"
      },
      "PEN": {
        "displayName-count-one": "பெரூவியன் சோல்",
        "displayName-count-other": "பெரூவியன் சோல்கள்"
      },
      "JPY": {
        "displayName-count-other": "ஜப்பானிய யென்"
      },
      "CNY": {
        "displayName-count-other": "சீன யுவான்"
      },
      "INR": {
        "displayName-count-one": "இந்திய ரூபாய்",
        "displayName-count-other": "இந்திய ரூபாய்கள்"
      },
      "KRW": {
        "displayName-count-other": "தென் கொரிய வான்"
      },
      "RUB": {
        "displayName-count-one": "ரஷியன் ரூபிள்",
        "displayName-count-other": "ரஷியன் ரூபிள்கள்"
      },
      "BRL": {
        "displayName-count-one": "பிரேசிலியன் ரியால்",
        "displayName-count-other": "பிரேசிலியன் ரியால்கள்"
      },
      "TRY": {
        "displayName-count-other": "துருக்கிஷ் லீரா"
      },
      "BDT": {
        "displayName-count-one": "பங்களாதேஷி டாகா",
        "displayName-count-other": "பங்களாதேஷி டாகாக்கள்"
      },
      "PKR": {
        "displayName-count-one": "பாகிஸ்தானி ரூபாய்",
        "displayName-count-other": "பாகிஸ்தானி ரூபாய்கள்"
      },
      "IDR": {
        "displayName-count-one": "இந்தோனேஷியன் ருபியா",
        "displayName-count-other": "இந்தோனேஷியன் ருபியாக்கள்"
      },
      "VND": {
        "displayName-count-other": "வியட்நாமீஸ் டாங்"
      }
    }
  },
  "vi": {
    "date_pattern": "d MMMM, y",
    "months": [
      "tháng 1",
      "tháng 2",
      "tháng 3",
      "tháng 4",
      "tháng 5",
      "tháng 6",
      "tháng 7",
      "tháng 8",
      "tháng 9",
      "tháng 10",
      "tháng 11",
      "tháng 12"
    ],
    "units": {
      "concentr-percent": {
        "unitPattern-count-other": "{0}%"
      },
      "duration-hour": {
        "unitPattern-count-other": "{0} tiếng"
      },
      "duration-minute": {
        "unitPattern-count-other": "{0} phút"
      },
      "duration-second": {
        "unitPattern-count-other": "{0} giây"
      },
      "length-meter": {
        "unitPattern-count-other": "{0} mét"
      },
      "length-kilometer": {
        "unitPattern-count-other": "{0} kilômét"
      },
      "mass-gram": {
        "unitPattern-count-other": "{0} gam"
      },
      "mass-kilogram": {
        "unitPattern-count-other": "{0} kilôgam"
      },
      "temperature-celsius": {
        "unitPattern-count-other": "{0} độ C"
      },
      "volume-liter": {
        "unitPattern-count-other": "{0} lít"
      },
      "volume-milliliter": {
        "unitPattern-count-other": "{0} mililít"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-other": "đô la Mỹ"
      },
      "EUR": {
        "displayName-count-other": "euro"
      },
      "GBP": {
        "displayName-count-other": "bảng Anh"
      },
      "PEN": {
        "displayName-count-other": "sol Peru"
      },
      "JPY": {
        "displayName-count-other": "yên Nhật"
      },
      "CNY": {
        "displayName-count-other": "nhân dân tệ"
      },
      "INR": {
        "displayName-count-other": "rupee Ấn Độ"
      },
      "KRW": {
        "displayName-count-other": "won Hàn Quốc"
      },
      "RUB": {
        "displayName-count-other": "rúp Nga"
      },
      "BRL": {
        "displayName-count-other": "real Braxin"
      },
      "TRY": {
        "displayName-count-other": "Lia Thổ Nhĩ Kỳ"
      },
      "BDT": {
        "displayName-count-other": "taka Bangladesh"
      },
      "PKR": {
        "displayName-count-other": "rupee Pakistan"
      },
      "IDR": {
        "displayName-count-other": "rupiah Indonesia"
      },
      "VND": {
        "displayName-count-other": "đồng Việt Nam"
      }
    }
  },
  "ko": {
    "date_pattern": "y년 MMMM d일",
    "months": [
      "1월",
      "2월",
      "3월",
      "4월",
      "5월",
      "6월",
      "7월",
      "8월",
      "9월",
      "10월",
      "11월",
      "12월"
    ],
    "units": {
      "concentr-percent": {
        "unitPattern-count-other": "{0}%"
      },
      "duration-hour": {
        "unitPattern-count-other": "{0}시간"
      },
      "duration-minute": {
        "unitPattern-count-other": "{0}분"
      },
      "duration-second": {
        "unitPattern-count-other": "{0}초"
      },
      "length-meter": {
        "unitPattern-count-other": "{0}미터"
      },
      "length-kilometer": {
        "unitPattern-count-other": "{0}킬로미터"
      },
      "mass-gram": {
        "unitPattern-count-other": "{0}그램"
      },
      "mass-kilogram": {
        "unitPattern-count-other": "{0}킬로그램"
      },
      "temperature-celsius": {
        "unitPattern-count-other": "섭씨 {0}도"
      },
      "volume-liter": {
        "unitPattern-count-other": "{0}리터"
      },
      "volume-milliliter": {
        "unitPattern-count-other": "{0}밀리리터"
      }
    },
    "currencies": {
      "USD": {
        "displayName-count-other": "미국 달러"
      },
      "EUR": {
        "displayName-count-other": "유로"
      },
      "GBP": {
        "displayName-count-other": "영국 파운드"
      },
      "PEN": {
        "displayName-count-other": "페루 솔"
      },
      "JPY": {
        "displayName-count-other": "일본 엔화"
      },
      "CNY": {
        "displayName-count-other": "중국 위안화"
      },
      "INR": {
        "displayName-count-other": "인도 루피"
      },
      "KRW": {
        "displayName-count-other": "대한민국 원"
      },
      "RUB": {
        "displayName-count-other": "러시아 루블"
      },
      "BRL": {
        "displayName-count-other": "브라질 레알"
      },
      "TRY": {
        "displayName-count-other": "튀르키예 리라"
      },
      "BDT": {
        "displayName-count-other": "방글라데시 타카"
      },
      "PKR": {
        "displayName-count-other": "파키스탄 루피"
      },
      "IDR": {
        "displayName-count-other": "인도네시아 루피아"
      },
      "VND": {
        "displayName-count-other": "베트남 동"
      }
    }
  }
}"#;
