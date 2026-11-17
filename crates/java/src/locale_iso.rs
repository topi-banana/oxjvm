//! `java.util.Locale` ISO 3166 country-code support: `getISOCountries`,
//! `getISO3Country`, and the `Locale.IsoCountryCode` enum.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{class, field, int, method, object, receiver, ref_arg};

// -------------------------------------------------------------------------------------------
// Locale.IsoCountryCode
// -------------------------------------------------------------------------------------------

const ISO_COUNTRY_CODE_NAMES: [&str; 3] = ["PART1_ALPHA2", "PART1_ALPHA3", "PART3"];

/// `java.util.Locale.IsoCountryCode`: selects the ISO 3166 code set returned by
/// `Locale.getISOCountries(IsoCountryCode)`.
pub(crate) const ISO_COUNTRY_CODE: oxjvm_vm::NativeClass = class(
    "java/util/Locale$IsoCountryCode",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_FINAL | ACC_ENUM,
    &[
        field("ordinal", "I", ACC_PRIVATE | ACC_FINAL, None),
        field(
            "PART1_ALPHA2",
            "Ljava/util/Locale$IsoCountryCode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "PART1_ALPHA3",
            "Ljava/util/Locale$IsoCountryCode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
        field(
            "PART3",
            "Ljava/util/Locale$IsoCountryCode;",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM,
            None,
        ),
    ],
    &ISO_COUNTRY_CODE_METHODS,
    Some(iso_country_code_clinit),
);

fn iso_country_code_clinit(vm: &mut Vm<'_>) -> Result<(), VmError> {
    let class = vm.resolve_class("java/util/Locale$IsoCountryCode")?;
    for (ordinal, name) in ISO_COUNTRY_CODE_NAMES.iter().enumerate() {
        let instance = vm.new_instance(class)?;
        if let Some((declaring, field)) = vm.find_field_opt(class, "ordinal", "I") {
            vm.set_instance_int(instance, declaring, field, ordinal as i32);
        }
        vm.set_static_value(
            class,
            name,
            "Ljava/util/Locale$IsoCountryCode;",
            Value::Ref(instance),
        );
    }
    Ok(())
}

const ISO_COUNTRY_CODE_METHODS: [oxjvm_vm::NativeMethodDef; 4] = [
    method("ordinal", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "ordinal"))
    }),
    method("name", "()Ljava/lang/String;", ACC_PUBLIC, |vm, _, args| {
        let index = crate::lang::read_int_field(vm, receiver(args), "ordinal").clamp(0, 2) as usize;
        object(vm.make_string(ISO_COUNTRY_CODE_NAMES[index])?)
    }),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let index =
                crate::lang::read_int_field(vm, receiver(args), "ordinal").clamp(0, 2) as usize;
            object(vm.make_string(ISO_COUNTRY_CODE_NAMES[index])?)
        },
    ),
    method(
        "values",
        "()[Ljava/util/Locale$IsoCountryCode;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, _| {
            let class = vm.resolve_class("java/util/Locale$IsoCountryCode")?;
            vm.ensure_initialized(class)?;
            let array = vm.allocate_object_array(class, ISO_COUNTRY_CODE_NAMES.len())?;
            for (index, name) in ISO_COUNTRY_CODE_NAMES.iter().enumerate() {
                if let Some(value) =
                    vm.read_static_ref_if_set(class, name, "Ljava/util/Locale$IsoCountryCode;")
                {
                    vm.array_set_ref(array, index, value)?;
                }
            }
            object(array)
        },
    ),
];

// -------------------------------------------------------------------------------------------
// Locale.getISOCountries / getISO3Country
// -------------------------------------------------------------------------------------------

/// `Locale.getISOCountries()`: the ISO 3166-1 alpha-2 codes, sorted.
pub(crate) fn locale_get_iso_countries(vm: &mut Vm<'_>) -> Result<Value, VmError> {
    let codes: Vec<String> = ISO_ALPHA2_ALPHA3
        .iter()
        .map(|(a2, _)| (*a2).to_string())
        .collect();
    crate::lang::string_array(vm, &codes)
}

/// `Locale.getISOCountries(IsoCountryCode)`: the selected ISO 3166 code set.
pub(crate) fn locale_get_iso_countries_typed(
    vm: &mut Vm<'_>,
    args: &[Value],
) -> Result<Value, VmError> {
    let code = ref_arg(args, 0);
    if code.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    let ordinal = crate::lang::read_int_field(vm, code, "ordinal");
    let texts: Vec<String> = match ordinal {
        1 => ISO_ALPHA2_ALPHA3
            .iter()
            .map(|(_, a3)| (*a3).to_string())
            .collect(),
        2 => ISO_PART3.iter().map(|c| (*c).to_string()).collect(),
        _ => ISO_ALPHA2_ALPHA3
            .iter()
            .map(|(a2, _)| (*a2).to_string())
            .collect(),
    };
    let mut elements: Vec<ObjectRef> = Vec::new();
    for text in &texts {
        elements.push(vm.make_string(text)?);
    }
    object(crate::util::hash_set_from(vm, &elements)?)
}

/// `Locale.getISO3Country()`: the ISO 3166-1 alpha-3 code for this locale's country.
pub(crate) fn locale_get_iso3_country(vm: &mut Vm<'_>, args: &[Value]) -> Result<Value, VmError> {
    let this = receiver(args);
    let country = vm
        .read_string_field(this, vm.class_of(this), "country")
        .unwrap_or_default();
    match ISO_ALPHA2_ALPHA3.iter().find(|(a2, _)| *a2 == country) {
        Some((_, a3)) => object(vm.make_string(a3)?),
        None => Err(vm.throw_new(
            "java/util/MissingResourceException",
            Some(&alloc::format!(
                "Couldn't find 3-letter country code for {country}"
            )),
        )),
    }
}

// -------------------------------------------------------------------------------------------
// ISO 3166 data (generated from the reference JDK)
// -------------------------------------------------------------------------------------------/// ISO 3166-1 alpha-2 to alpha-3 country codes (249 entries).
static ISO_ALPHA2_ALPHA3: [(&str, &str); 249] = [
    ("AD", "AND"),
    ("AE", "ARE"),
    ("AF", "AFG"),
    ("AG", "ATG"),
    ("AI", "AIA"),
    ("AL", "ALB"),
    ("AM", "ARM"),
    ("AO", "AGO"),
    ("AQ", "ATA"),
    ("AR", "ARG"),
    ("AS", "ASM"),
    ("AT", "AUT"),
    ("AU", "AUS"),
    ("AW", "ABW"),
    ("AX", "ALA"),
    ("AZ", "AZE"),
    ("BA", "BIH"),
    ("BB", "BRB"),
    ("BD", "BGD"),
    ("BE", "BEL"),
    ("BF", "BFA"),
    ("BG", "BGR"),
    ("BH", "BHR"),
    ("BI", "BDI"),
    ("BJ", "BEN"),
    ("BL", "BLM"),
    ("BM", "BMU"),
    ("BN", "BRN"),
    ("BO", "BOL"),
    ("BQ", "BES"),
    ("BR", "BRA"),
    ("BS", "BHS"),
    ("BT", "BTN"),
    ("BV", "BVT"),
    ("BW", "BWA"),
    ("BY", "BLR"),
    ("BZ", "BLZ"),
    ("CA", "CAN"),
    ("CC", "CCK"),
    ("CD", "COD"),
    ("CF", "CAF"),
    ("CG", "COG"),
    ("CH", "CHE"),
    ("CI", "CIV"),
    ("CK", "COK"),
    ("CL", "CHL"),
    ("CM", "CMR"),
    ("CN", "CHN"),
    ("CO", "COL"),
    ("CR", "CRI"),
    ("CU", "CUB"),
    ("CV", "CPV"),
    ("CW", "CUW"),
    ("CX", "CXR"),
    ("CY", "CYP"),
    ("CZ", "CZE"),
    ("DE", "DEU"),
    ("DJ", "DJI"),
    ("DK", "DNK"),
    ("DM", "DMA"),
    ("DO", "DOM"),
    ("DZ", "DZA"),
    ("EC", "ECU"),
    ("EE", "EST"),
    ("EG", "EGY"),
    ("EH", "ESH"),
    ("ER", "ERI"),
    ("ES", "ESP"),
    ("ET", "ETH"),
    ("FI", "FIN"),
    ("FJ", "FJI"),
    ("FK", "FLK"),
    ("FM", "FSM"),
    ("FO", "FRO"),
    ("FR", "FRA"),
    ("GA", "GAB"),
    ("GB", "GBR"),
    ("GD", "GRD"),
    ("GE", "GEO"),
    ("GF", "GUF"),
    ("GG", "GGY"),
    ("GH", "GHA"),
    ("GI", "GIB"),
    ("GL", "GRL"),
    ("GM", "GMB"),
    ("GN", "GIN"),
    ("GP", "GLP"),
    ("GQ", "GNQ"),
    ("GR", "GRC"),
    ("GS", "SGS"),
    ("GT", "GTM"),
    ("GU", "GUM"),
    ("GW", "GNB"),
    ("GY", "GUY"),
    ("HK", "HKG"),
    ("HM", "HMD"),
    ("HN", "HND"),
    ("HR", "HRV"),
    ("HT", "HTI"),
    ("HU", "HUN"),
    ("ID", "IDN"),
    ("IE", "IRL"),
    ("IL", "ISR"),
    ("IM", "IMN"),
    ("IN", "IND"),
    ("IO", "IOT"),
    ("IQ", "IRQ"),
    ("IR", "IRN"),
    ("IS", "ISL"),
    ("IT", "ITA"),
    ("JE", "JEY"),
    ("JM", "JAM"),
    ("JO", "JOR"),
    ("JP", "JPN"),
    ("KE", "KEN"),
    ("KG", "KGZ"),
    ("KH", "KHM"),
    ("KI", "KIR"),
    ("KM", "COM"),
    ("KN", "KNA"),
    ("KP", "PRK"),
    ("KR", "KOR"),
    ("KW", "KWT"),
    ("KY", "CYM"),
    ("KZ", "KAZ"),
    ("LA", "LAO"),
    ("LB", "LBN"),
    ("LC", "LCA"),
    ("LI", "LIE"),
    ("LK", "LKA"),
    ("LR", "LBR"),
    ("LS", "LSO"),
    ("LT", "LTU"),
    ("LU", "LUX"),
    ("LV", "LVA"),
    ("LY", "LBY"),
    ("MA", "MAR"),
    ("MC", "MCO"),
    ("MD", "MDA"),
    ("ME", "MNE"),
    ("MF", "MAF"),
    ("MG", "MDG"),
    ("MH", "MHL"),
    ("MK", "MKD"),
    ("ML", "MLI"),
    ("MM", "MMR"),
    ("MN", "MNG"),
    ("MO", "MAC"),
    ("MP", "MNP"),
    ("MQ", "MTQ"),
    ("MR", "MRT"),
    ("MS", "MSR"),
    ("MT", "MLT"),
    ("MU", "MUS"),
    ("MV", "MDV"),
    ("MW", "MWI"),
    ("MX", "MEX"),
    ("MY", "MYS"),
    ("MZ", "MOZ"),
    ("NA", "NAM"),
    ("NC", "NCL"),
    ("NE", "NER"),
    ("NF", "NFK"),
    ("NG", "NGA"),
    ("NI", "NIC"),
    ("NL", "NLD"),
    ("NO", "NOR"),
    ("NP", "NPL"),
    ("NR", "NRU"),
    ("NU", "NIU"),
    ("NZ", "NZL"),
    ("OM", "OMN"),
    ("PA", "PAN"),
    ("PE", "PER"),
    ("PF", "PYF"),
    ("PG", "PNG"),
    ("PH", "PHL"),
    ("PK", "PAK"),
    ("PL", "POL"),
    ("PM", "SPM"),
    ("PN", "PCN"),
    ("PR", "PRI"),
    ("PS", "PSE"),
    ("PT", "PRT"),
    ("PW", "PLW"),
    ("PY", "PRY"),
    ("QA", "QAT"),
    ("RE", "REU"),
    ("RO", "ROU"),
    ("RS", "SRB"),
    ("RU", "RUS"),
    ("RW", "RWA"),
    ("SA", "SAU"),
    ("SB", "SLB"),
    ("SC", "SYC"),
    ("SD", "SDN"),
    ("SE", "SWE"),
    ("SG", "SGP"),
    ("SH", "SHN"),
    ("SI", "SVN"),
    ("SJ", "SJM"),
    ("SK", "SVK"),
    ("SL", "SLE"),
    ("SM", "SMR"),
    ("SN", "SEN"),
    ("SO", "SOM"),
    ("SR", "SUR"),
    ("SS", "SSD"),
    ("ST", "STP"),
    ("SV", "SLV"),
    ("SX", "SXM"),
    ("SY", "SYR"),
    ("SZ", "SWZ"),
    ("TC", "TCA"),
    ("TD", "TCD"),
    ("TF", "ATF"),
    ("TG", "TGO"),
    ("TH", "THA"),
    ("TJ", "TJK"),
    ("TK", "TKL"),
    ("TL", "TLS"),
    ("TM", "TKM"),
    ("TN", "TUN"),
    ("TO", "TON"),
    ("TR", "TUR"),
    ("TT", "TTO"),
    ("TV", "TUV"),
    ("TW", "TWN"),
    ("TZ", "TZA"),
    ("UA", "UKR"),
    ("UG", "UGA"),
    ("UM", "UMI"),
    ("US", "USA"),
    ("UY", "URY"),
    ("UZ", "UZB"),
    ("VA", "VAT"),
    ("VC", "VCT"),
    ("VE", "VEN"),
    ("VG", "VGB"),
    ("VI", "VIR"),
    ("VN", "VNM"),
    ("VU", "VUT"),
    ("WF", "WLF"),
    ("WS", "WSM"),
    ("YE", "YEM"),
    ("YT", "MYT"),
    ("ZA", "ZAF"),
    ("ZM", "ZMB"),
    ("ZW", "ZWE"),
];

/// ISO 3166-3 (formerly used) alpha-4 codes (31 entries).
static ISO_PART3: [&str; 31] = [
    "AIDJ", "ANHH", "BQAQ", "BUMM", "BYAA", "CSHH", "CSXX", "CTKI", "DDDE", "DYBJ", "FQHH", "FXFR",
    "GEHH", "HVBF", "JTUM", "MIUM", "NHVU", "NQAQ", "NTHH", "PCHH", "PUUM", "PZPA", "RHZW", "SKIN",
    "SUHH", "TPTL", "VDVN", "WKUM", "YDYE", "YUCS", "ZRCD",
];
