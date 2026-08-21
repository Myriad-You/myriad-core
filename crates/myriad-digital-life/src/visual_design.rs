use serde_json::{Map, Value};

/// Required visual-identity fields for a close upper-body companion portrait.
/// Lower-body garments, footwear, and articulated limb design intentionally do
/// not belong to this contract.
pub const UPPER_BODY_VISUAL_IDENTITY_FIELDS: [(&str, usize); 11] = [
    ("faceDesign", 500),
    ("eyeDesign", 500),
    ("hairShape", 500),
    ("hairLayerPlan", 700),
    ("upperBodySilhouette", 700),
    ("outfitConstruction", 1_200),
    ("sleeveArmDesign", 700),
    ("materialPlan", 1_200),
    ("heroAccessory", 500),
    ("paletteHint", 500),
    ("motif", 500),
];

pub fn sanitize_upper_body_visual_identity(value: &Value) -> Option<Value> {
    let source = value
        .get("visualIdentity")
        .unwrap_or(value)
        .as_object()?;
    let mut sanitized = Map::new();
    for (key, max_chars) in UPPER_BODY_VISUAL_IDENTITY_FIELDS {
        let raw = source.get(key)?.as_str()?.trim();
        if raw.is_empty()
            || raw.chars().count() > max_chars
            || raw.chars().any(char::is_control)
        {
            return None;
        }
        sanitized.insert(key.to_string(), Value::String(raw.to_string()));
    }
    Some(Value::Object(sanitized))
}

pub fn upper_body_visual_identity_is_complete(value: &Value) -> bool {
    sanitize_upper_body_visual_identity(value).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn complete_design() -> Value {
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
        let sanitized = sanitize_upper_body_visual_identity(&complete_design()).unwrap();
        assert_eq!(sanitized.as_object().unwrap().len(), 11);
        assert!(upper_body_visual_identity_is_complete(&sanitized));
        assert!(sanitized.get("footwear").is_none());
    }

    #[test]
    fn missing_or_control_text_rejects_the_design() {
        let mut missing = complete_design();
        missing["visualIdentity"].as_object_mut().unwrap().remove("eyeDesign");
        assert!(!upper_body_visual_identity_is_complete(&missing));
        let mut control = complete_design();
        control["visualIdentity"]["motif"] = json!("星轨\u{0000}");
        assert!(!upper_body_visual_identity_is_complete(&control));
    }
}
