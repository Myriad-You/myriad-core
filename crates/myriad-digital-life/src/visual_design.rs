use serde_json::{json, Map, Value};

/// Required visual-identity fields for a close upper-body companion portrait.
/// Lower-body garments, footwear, and articulated limb design intentionally do
/// not belong to this contract.
pub const CLOTHING_STYLES: [&str; 17] = [
    "everyday",
    "uniform",
    "fantasy",
    "urban",
    "east-asian",
    "japanese",
    "sci-fi",
    "formal",
    "sport",
    "idol",
    "gothic",
    "lounge",
    "royal",
    "mystic",
    "travel",
    "vintage",
    "rain",
];

pub const CHARACTER_VISUAL_FIELDS: [(&str, usize); 4] = [
    ("faceDesign", 500),
    ("eyeDesign", 500),
    ("hairShape", 500),
    ("hairLayerPlan", 700),
];

pub const OUTFIT_VISUAL_FIELDS: [(&str, usize); 7] = [
    ("upperBodySilhouette", 700),
    ("outfitConstruction", 1_200),
    ("sleeveArmDesign", 700),
    ("materialPlan", 1_200),
    ("heroAccessory", 500),
    ("paletteHint", 500),
    ("motif", 500),
];

/// Flat field list for prompts and language checks. Order is character then outfit.
pub const UPPER_BODY_VISUAL_IDENTITY_FIELDS: [(&str, usize); 11] = [
    CHARACTER_VISUAL_FIELDS[0],
    CHARACTER_VISUAL_FIELDS[1],
    CHARACTER_VISUAL_FIELDS[2],
    CHARACTER_VISUAL_FIELDS[3],
    OUTFIT_VISUAL_FIELDS[0],
    OUTFIT_VISUAL_FIELDS[1],
    OUTFIT_VISUAL_FIELDS[2],
    OUTFIT_VISUAL_FIELDS[3],
    OUTFIT_VISUAL_FIELDS[4],
    OUTFIT_VISUAL_FIELDS[5],
    OUTFIT_VISUAL_FIELDS[6],
];

pub fn normalize_clothing_style(raw: &str) -> Option<&'static str> {
    let value = raw.trim();
    CLOTHING_STYLES.iter().copied().find(|id| *id == value)
}

/// Stable costume grammar for the design model. Language must not substitute this.
pub fn clothing_style_grammar(id: &str) -> Option<&'static str> {
    Some(match normalize_clothing_style(id)? {
        "everyday" => {
            "contemporary casual everyday clothes. Invent a new cut each time. Not historical dress, not a national costume, not court crests or frog-button hardware."
        }
        "uniform" => {
            "school, academy, or service uniform. Invent a new cut each time — not always a sailor collar and neckerchief."
        }
        "fantasy" => {
            "original fantasy-adventure costume. Invent a new cut each time — not always a capelet and one chest brooch. Not locked to any real-world nation or dynasty."
        }
        "urban" => {
            "contemporary city clothes or smart streetwear. Invent a new cut each time — not always a hoodie and zipper."
        }
        "east-asian" => {
            "Chinese-inspired layered traditional or modern-hanfu fusion, only because the owner chose it. Invent a new cut each time — not always the same cross-collar and frog buttons."
        }
        "japanese" => {
            "Japanese-inspired traditional, shrine, or modern-wa fusion, only because the owner chose it. Invent a new cut each time — not always a shrine-maiden collar and rope."
        }
        "sci-fi" => {
            "science-fiction or futurist costume. Invent a new cut each time — not always white plates and a chest light."
        }
        "formal" => {
            "contemporary formalwear or evening tailoring. Invent a new cut each time. Not a wedding-dress cliché, not idol stage badges or bows."
        }
        "sport" => {
            "athletic or outdoor sport topwear. Invent a new cut each time — not always a track jacket with a chest logo. Not stage jewelry, not court metalwork."
        }
        "royal" => {
            "original court or royal ceremonial costume. Invent a new cut each time — not always a gold crest and a tiara. Not a real dynasty uniform, not idol stagewear."
        }
        "idol" => {
            "original idol or stage performance costume. Stay in live-stage / performance wear. Invent a new cut each time — not always a cropped jacket with bows and an ear headset. Not court ceremonial dress, not frog-button or hanfu hardware, not a real group uniform."
        }
        "gothic" => {
            "contemporary gothic or dark-romantic costume. Invent a new cut each time — not always a black lace collar and one choker."
        }
        "lounge" => {
            "soft indoor loungewear or knit home clothes. Invent a new cut each time — not always an open cardigan over a camisole."
        }
        "mystic" => {
            "original mystic, occult, or ritual costume. Invent a new cut each time — not always a hooded cloak and one amulet. Not clerical dress of a real faith."
        }
        "travel" => {
            "layered traveler or expedition outerwear. Invent a new cut each time — not always a utility vest and one satchel strap."
        }
        "vintage" => {
            "vintage or retro mid-century civilian clothes. Invent a new cut each time — not always a peter-pan collar and one brooch."
        }
        "rain" => {
            "rain-coat or trench family outerwear. Invent a new cut each time — not always the same storm-flap trench."
        }
        _ => return None,
    })
}

pub fn sanitize_upper_body_visual_identity(value: &Value) -> Option<Value> {
    sanitize_modular_identity(value).or_else(|| wrap_flat_identity(value))
}

pub fn upper_body_visual_identity_is_complete(value: &Value) -> bool {
    sanitize_upper_body_visual_identity(value).is_some()
}

pub fn flatten_visual_identity(value: &Value) -> Option<Value> {
    let modular = sanitize_upper_body_visual_identity(value)?;
    let mut flat = Map::new();
    copy_module_fields(
        &mut flat,
        modular.get("character")?.as_object()?,
        &CHARACTER_VISUAL_FIELDS,
    )?;
    copy_module_fields(
        &mut flat,
        modular.get("outfit")?.as_object()?,
        &OUTFIT_VISUAL_FIELDS,
    )?;
    Some(Value::Object(flat))
}

pub fn character_module(value: &Value) -> Option<Value> {
    sanitize_upper_body_visual_identity(value)?
        .get("character")
        .cloned()
}

pub fn outfit_module(value: &Value) -> Option<Value> {
    sanitize_upper_body_visual_identity(value)?
        .get("outfit")
        .cloned()
}

pub fn clothing_style_of(value: &Value) -> Option<&'static str> {
    let root = value.get("visualIdentity").unwrap_or(value);
    [
        value.get("clothingStyle"),
        root.get("clothingStyle"),
        root
            .get("outfit")
            .and_then(|outfit| outfit.get("clothingStyle")),
    ]
    .into_iter()
    .flatten()
    .find_map(Value::as_str)
    .and_then(normalize_clothing_style)
}

pub fn stamp_clothing_style(identity: &mut Value, style: &str) -> Option<&'static str> {
    let style = normalize_clothing_style(style)?;
    identity
        .get_mut("outfit")
        .and_then(Value::as_object_mut)?
        .insert("clothingStyle".into(), json!(style));
    Some(style)
}

fn sanitize_modular_identity(value: &Value) -> Option<Value> {
    let root = value.get("visualIdentity").unwrap_or(value);
    let character = sanitize_fields(root.get("character")?, &CHARACTER_VISUAL_FIELDS)?;
    let mut outfit = sanitize_fields(root.get("outfit")?, &OUTFIT_VISUAL_FIELDS)?;
    if let Some(style) = root
        .get("outfit")
        .and_then(|outfit| outfit.get("clothingStyle"))
        .and_then(Value::as_str)
        .and_then(normalize_clothing_style)
    {
        outfit
            .as_object_mut()?
            .insert("clothingStyle".into(), json!(style));
    }
    Some(json!({
        "character": character,
        "outfit": outfit,
    }))
}

fn wrap_flat_identity(value: &Value) -> Option<Value> {
    let source = value.get("visualIdentity").unwrap_or(value);
    let character = sanitize_fields(source, &CHARACTER_VISUAL_FIELDS)?;
    let mut outfit = sanitize_fields(source, &OUTFIT_VISUAL_FIELDS)?;
    if let Some(style) = source
        .get("clothingStyle")
        .and_then(Value::as_str)
        .and_then(normalize_clothing_style)
        .or_else(|| {
            value
                .get("clothingStyle")
                .and_then(Value::as_str)
                .and_then(normalize_clothing_style)
        })
    {
        outfit
            .as_object_mut()?
            .insert("clothingStyle".into(), json!(style));
    }
    Some(json!({
        "character": character,
        "outfit": outfit,
    }))
}

fn sanitize_fields(source: &Value, fields: &[(&str, usize)]) -> Option<Value> {
    let source = source.as_object()?;
    let mut sanitized = Map::new();
    for (key, max_chars) in fields {
        let raw = source.get(*key)?.as_str()?.trim();
        if raw.is_empty()
            || raw.chars().count() > *max_chars
            || raw.chars().any(char::is_control)
        {
            return None;
        }
        sanitized.insert((*key).to_string(), Value::String(raw.to_string()));
    }
    Some(Value::Object(sanitized))
}

fn copy_module_fields(
    target: &mut Map<String, Value>,
    source: &Map<String, Value>,
    fields: &[(&str, usize)],
) -> Option<()> {
    for (key, _) in fields {
        target.insert((*key).to_string(), source.get(*key)?.clone());
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn complete_flat() -> Value {
        json!({
            "visualIdentity": {
                "faceDesign": "柔和的鹅蛋脸，鼻唇简洁，面部比例成熟而非幼态",
                "eyeDesign": "紫蓝宝石感大眼，深色上睫与多层虹膜高光",
                "hairShape": "粉色齐颌短发，空气刘海，侧发包住脸颊",
                "hairLayerPlan": "后发形成完整轮廓，前刘海、左右侧发和顶部呆毛可分层",
                "upperBodySilhouette": "窄肩与清晰领口，胸像轮廓紧凑，左右袖片伸入画面",
                "outfitConstruction": "水手领内搭叠短外套，领巾形成胸前主形，结构止于高腰",
                "sleeveArmDesign": "宽松袖口包住局部前臂，左右形状不完全对称，手可以不出现",
                "materialPlan": "哑光布料为主，丝带带柔和光泽，金属与宝石只用于小面积焦点",
                "heroAccessory": "左侧星形发夹与胸前星形扣形成一次呼应",
                "paletteHint": "粉色头发，淡紫与白为主体，深紫压边，少量金色点缀",
                "motif": "星轨与小型鸟笼，集中在发饰和胸前，不铺满服装"
            }
        })
    }

    #[test]
    fn complete_upper_body_design_is_normalized() {
        let sanitized = sanitize_upper_body_visual_identity(&complete_flat()).unwrap();
        assert!(sanitized.get("character").is_some());
        assert!(sanitized.get("outfit").is_some());
        assert!(sanitized.get("footwear").is_none());
        assert!(upper_body_visual_identity_is_complete(&sanitized));
        let flat = flatten_visual_identity(&sanitized).unwrap();
        assert_eq!(flat.as_object().unwrap().len(), 11);
        assert_eq!(flat["hairShape"], "粉色齐颌短发，空气刘海，侧发包住脸颊");
        assert_eq!(
            flat["outfitConstruction"],
            "水手领内搭叠短外套，领巾形成胸前主形，结构止于高腰"
        );
    }

    #[test]
    fn modular_identity_round_trips() {
        let modular = json!({
            "character": complete_flat()["visualIdentity"].as_object().unwrap()
                .iter()
                .filter(|(key, _)| CHARACTER_VISUAL_FIELDS.iter().any(|(field, _)| field == key))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<Map<_, _>>(),
            "outfit": {
                "clothingStyle": "everyday",
                "upperBodySilhouette": complete_flat()["visualIdentity"]["upperBodySilhouette"],
                "outfitConstruction": complete_flat()["visualIdentity"]["outfitConstruction"],
                "sleeveArmDesign": complete_flat()["visualIdentity"]["sleeveArmDesign"],
                "materialPlan": complete_flat()["visualIdentity"]["materialPlan"],
                "heroAccessory": complete_flat()["visualIdentity"]["heroAccessory"],
                "paletteHint": complete_flat()["visualIdentity"]["paletteHint"],
                "motif": complete_flat()["visualIdentity"]["motif"]
            }
        });
        let sanitized = sanitize_upper_body_visual_identity(&modular).unwrap();
        assert_eq!(sanitized["outfit"]["clothingStyle"], "everyday");
        assert_eq!(
            flatten_visual_identity(&modular).unwrap()["faceDesign"],
            complete_flat()["visualIdentity"]["faceDesign"]
        );
    }

    #[test]
    fn missing_or_control_text_rejects_the_design() {
        let mut missing = complete_flat();
        missing["visualIdentity"].as_object_mut().unwrap().remove("eyeDesign");
        assert!(!upper_body_visual_identity_is_complete(&missing));
        let mut control = complete_flat();
        control["visualIdentity"]["motif"] = json!("星轨\u{0000}");
        assert!(!upper_body_visual_identity_is_complete(&control));
    }

    #[test]
    fn clothing_style_is_an_explicit_opt_in() {
        assert_eq!(normalize_clothing_style(" fantasy "), Some("fantasy"));
        assert_eq!(normalize_clothing_style("idol"), Some("idol"));
        assert_eq!(normalize_clothing_style("国风"), None);
        assert_eq!(normalize_clothing_style("zh-CN"), None);
        let grammar = clothing_style_grammar("everyday").unwrap();
        assert!(grammar.contains("Not historical"));
        assert!(clothing_style_grammar("east-asian")
            .unwrap()
            .contains("only because the owner chose it"));
        assert!(clothing_style_grammar("royal")
            .unwrap()
            .contains("Not a real dynasty"));
        assert!(clothing_style_grammar("idol")
            .unwrap()
            .contains("Not court ceremonial dress"));
        assert!(clothing_style_grammar("idol")
            .unwrap()
            .contains("not always a cropped jacket"));
        assert!(clothing_style_grammar("uniform")
            .unwrap()
            .contains("Invent a new cut each time"));
        assert!(clothing_style_grammar("rain")
            .unwrap()
            .contains("not always the same storm-flap trench"));
    }

    #[test]
    fn clothing_style_survives_on_the_outfit_module() {
        let mut identity = sanitize_upper_body_visual_identity(&complete_flat()).unwrap();
        assert_eq!(stamp_clothing_style(&mut identity, "sci-fi"), Some("sci-fi"));
        assert_eq!(identity["outfit"]["clothingStyle"], "sci-fi");
        assert_eq!(clothing_style_of(&identity), Some("sci-fi"));
        assert_eq!(
            clothing_style_of(&json!({
                "clothingStyle": "urban",
                "visualIdentity": identity
            })),
            Some("urban")
        );
    }

    #[test]
    fn character_module_stays_when_outfit_changes() {
        let first = sanitize_upper_body_visual_identity(&complete_flat()).unwrap();
        let character = character_module(&first).unwrap();
        let mut swapped = first.clone();
        swapped["outfit"]["outfitConstruction"] =
            json!("高领内搭叠短风衣，胸前只有一条结构线，止于高腰");
        stamp_clothing_style(&mut swapped, "urban");
        assert_eq!(character_module(&swapped).unwrap(), character);
        assert_ne!(
            outfit_module(&swapped).unwrap()["outfitConstruction"],
            first["outfit"]["outfitConstruction"]
        );
        assert_eq!(clothing_style_of(&swapped), Some("urban"));
    }
}
