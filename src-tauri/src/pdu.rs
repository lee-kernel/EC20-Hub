use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DecodedPdu {
    pub sender: String,
    pub timestamp: String,
    pub text: String,
    pub concat: Option<ConcatInfo>,
}

#[derive(Debug, Clone)]
pub struct ConcatInfo {
    pub ref_id: u16,
    pub total: u8,
    pub seq: u8,
}

const GSM_7BIT_BASIC: [char; 128] = [
    '@', '£', '$', '¥', 'è', 'é', 'ù', 'ì', 'ò', 'Ç', '\n', 'Ø', 'ø', '\r', 'Å', 'å',
    'Δ', '_', 'Φ', 'Γ', 'Λ', 'Ω', 'Π', 'Ψ', 'Σ', 'Θ', 'Ξ', '\x1b', 'Æ', 'æ', 'ß', 'É',
    ' ', '!', '"', '#', '¤', '%', '&', '\'', '(', ')', '*', '+', ',', '-', '.', '/',
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', ':', ';', '<', '=', '>', '?',
    '¡', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O',
    'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', 'Ä', 'Ö', 'Ñ', 'Ü', '§',
    '¿', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o',
    'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', 'ä', 'ö', 'ñ', 'ü', 'à',
];

fn gsm_7bit_ext(code: u8) -> Option<char> {
    match code {
        0x0A => Some('\x0C'),
        0x14 => Some('^'),
        0x28 => Some('{'),
        0x29 => Some('}'),
        0x2F => Some('\\'),
        0x3C => Some('['),
        0x3D => Some('~'),
        0x3E => Some(']'),
        0x40 => Some('|'),
        0x65 => Some('€'),
        _ => None,
    }
}

fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, String> {
    let clean: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
    if clean.len() % 2 != 0 {
        return Err("Odd-length PDU hex string".to_string());
    }
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

fn decode_bcd_number(bytes: &[u8], num_digits: usize, toa: u8) -> String {
    let is_international = (toa & 0x70) == 0x10;
    let is_alphanumeric = (toa & 0x70) == 0x50;

    if is_alphanumeric {
        // Alphanumeric sender is GSM 7-bit packed; num_digits is semi-octets count
        let septets = (num_digits * 4) / 7;
        return decode_gsm7_bytes(bytes, septets, 0);
    }

    let mut out = String::new();
    if is_international {
        out.push('+');
    }
    for &b in bytes {
        let low = b & 0x0F;
        let high = (b >> 4) & 0x0F;
        if low <= 9 {
            out.push((b'0' + low) as char);
        }
        if high <= 9 && out.len() < num_digits + usize::from(is_international) {
            out.push((b'0' + high) as char);
        }
    }
    out
}

fn swap_semi_octet(b: u8) -> u8 {
    let low = b & 0x0F;
    let high = (b >> 4) & 0x0F;
    low * 10 + high
}

fn decode_scts(bytes: &[u8]) -> String {
    if bytes.len() < 7 {
        return String::new();
    }
    let year = 2000 + swap_semi_octet(bytes[0]) as u16;
    let month = swap_semi_octet(bytes[1]);
    let day = swap_semi_octet(bytes[2]);
    let hour = swap_semi_octet(bytes[3]);
    let min = swap_semi_octet(bytes[4]);
    let sec = swap_semi_octet(bytes[5]);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{min:02}:{sec:02}")
}

fn decode_gsm7_bytes(data: &[u8], total_septets: usize, fill_bits: usize) -> String {
    let mut out = String::new();
    let mut escape = false;
    for i in 0..total_septets {
        let bit_pos = fill_bits + i * 7;
        let byte_idx = bit_pos / 8;
        let bit_offset = bit_pos % 8;
        if byte_idx >= data.len() {
            break;
        }
        let mut val = (data[byte_idx] >> bit_offset) & 0x7F;
        if bit_offset > 1 && byte_idx + 1 < data.len() {
            val |= (data[byte_idx + 1] << (8 - bit_offset)) & 0x7F;
        }
        if escape {
            escape = false;
            if let Some(ch) = gsm_7bit_ext(val) {
                out.push(ch);
            }
        } else if val == 0x1B {
            escape = true;
        } else if (val as usize) < GSM_7BIT_BASIC.len() {
            out.push(GSM_7BIT_BASIC[val as usize]);
        }
    }
    out
}

fn decode_ucs2(data: &[u8]) -> String {
    let units: Vec<u16> = data
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

pub fn decode_pdu(hex: &str) -> Result<DecodedPdu, String> {
    let bytes = hex_to_bytes(hex)?;
    if bytes.is_empty() {
        return Err("Empty PDU".to_string());
    }

    let mut pos = 0;
    let smsc_len = bytes[pos] as usize;
    pos += 1 + smsc_len;
    if pos >= bytes.len() {
        return Err("Truncated PDU after SMSC".to_string());
    }

    let first_octet = bytes[pos];
    pos += 1;
    let udhi = (first_octet & 0x40) != 0;
    let mti = first_octet & 0x03;
    if mti != 0x00 && mti != 0x02 {
        // 0x00 = SMS-DELIVER, 0x02 = SMS-STATUS-REPORT (treat best effort)
    }

    if pos + 2 > bytes.len() {
        return Err("Truncated sender length".to_string());
    }
    let oa_len = bytes[pos] as usize; // number of semi-octets
    pos += 1;
    let oa_toa = bytes[pos];
    pos += 1;
    let oa_byte_len = (oa_len + 1) / 2;
    if pos + oa_byte_len > bytes.len() {
        return Err("Truncated sender address".to_string());
    }
    let sender = decode_bcd_number(&bytes[pos..pos + oa_byte_len], oa_len, oa_toa);
    pos += oa_byte_len;

    if pos + 2 > bytes.len() {
        return Err("Truncated PID/DCS".to_string());
    }
    let _pid = bytes[pos];
    let dcs = bytes[pos + 1];
    pos += 2;

    if pos + 7 > bytes.len() {
        return Err("Truncated SCTS".to_string());
    }
    let timestamp = decode_scts(&bytes[pos..pos + 7]);
    pos += 7;

    if pos >= bytes.len() {
        return Err("Truncated UDL".to_string());
    }
    let udl = bytes[pos] as usize;
    pos += 1;
    let ud = &bytes[pos..];

    let mut concat: Option<ConcatInfo> = None;
    let mut udh_byte_len = 0usize;
    if udhi && !ud.is_empty() {
        let udhl = ud[0] as usize;
        udh_byte_len = 1 + udhl;
        if udh_byte_len <= ud.len() {
            let mut ie_pos = 1;
            while ie_pos + 1 < udh_byte_len {
                let iei = ud[ie_pos];
                let iedl = ud[ie_pos + 1] as usize;
                let ie_data_start = ie_pos + 2;
                if ie_data_start + iedl <= udh_byte_len {
                    if iei == 0x00 && iedl == 3 {
                        concat = Some(ConcatInfo {
                            ref_id: ud[ie_data_start] as u16,
                            total: ud[ie_data_start + 1],
                            seq: ud[ie_data_start + 2],
                        });
                    } else if iei == 0x08 && iedl == 4 {
                        concat = Some(ConcatInfo {
                            ref_id: u16::from_be_bytes([ud[ie_data_start], ud[ie_data_start + 1]]),
                            total: ud[ie_data_start + 2],
                            seq: ud[ie_data_start + 3],
                        });
                    }
                }
                ie_pos += 2 + iedl;
            }
        } else {
            udh_byte_len = 0;
        }
    }

    // DCS coding: bits 3..2: 00 = 7-bit, 01 = 8-bit, 10 = UCS2
    let alphabet = if (dcs & 0xC0) == 0x00 || (dcs & 0xF0) == 0xF0 {
        (dcs >> 2) & 0x03
    } else {
        (dcs >> 2) & 0x03
    };

    let text = match alphabet {
        0x02 => {
            let slice = if udh_byte_len < ud.len() {
                &ud[udh_byte_len..]
            } else {
                &[]
            };
            decode_ucs2(slice)
        }
        0x01 => {
            let slice = if udh_byte_len < ud.len() {
                &ud[udh_byte_len..]
            } else {
                &[]
            };
            String::from_utf8_lossy(slice).to_string()
        }
        _ => {
            let total_udh_bits = udh_byte_len * 8;
            let skip_septets = (total_udh_bits + 6) / 7;
            let fill_bits = (skip_septets * 7) - total_udh_bits;
            let payload_septets = udl.saturating_sub(skip_septets);
            let slice = if udh_byte_len < ud.len() {
                &ud[udh_byte_len..]
            } else {
                &[]
            };
            decode_gsm7_bytes(slice, payload_septets, fill_bits)
        }
    };

    Ok(DecodedPdu {
        sender,
        timestamp,
        text,
        concat,
    })
}

/// Encode a single SMS-SUBMIT PDU in UCS2 (supports Chinese and all Unicode up to 70 chars)
pub fn encode_ucs2_submit_pdu(recipient: &str, text: &str) -> Result<(usize, String), String> {
    let mut clean_num = recipient.trim().to_string();
    let is_intl = clean_num.starts_with('+');
    if is_intl {
        clean_num.remove(0);
    }
    let digits: String = clean_num.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return Err("无效的收件人号码".to_string());
    }

    let utf16: Vec<u16> = text.encode_utf16().collect();
    if utf16.len() > 70 {
        return Err("单条 UCS2 短信最多支持 70 个字符".to_string());
    }

    let mut pdu = Vec::new();
    // SMSC length = 0 (use SIM default SMSC)
    pdu.push(0x00);
    // PDU type: SMS-SUBMIT (0x11 = VP relative)
    pdu.push(0x11);
    // TP-MR
    pdu.push(0x00);
    // Destination address length (digits count)
    pdu.push(digits.len() as u8);
    // TOA
    pdu.push(if is_intl { 0x91 } else { 0x81 });
    // BCD digits
    let digit_bytes: Vec<u8> = digits.bytes().map(|b| b - b'0').collect();
    for pair in digit_bytes.chunks(2) {
        let low = pair[0];
        let high = if pair.len() > 1 { pair[1] } else { 0x0F };
        pdu.push((high << 4) | low);
    }
    // PID
    pdu.push(0x00);
    // DCS = 0x08 (UCS2)
    pdu.push(0x08);
    // VP = 0xAA (4 days)
    pdu.push(0xAA);
    // UDL (bytes)
    pdu.push((utf16.len() * 2) as u8);
    for u in utf16 {
        pdu.extend_from_slice(&u.to_be_bytes());
    }

    // AT+CMGS length excludes the initial SMSC length byte (1 byte)
    let tpdu_len = pdu.len() - 1;
    let hex: String = pdu.iter().map(|b| format!("{b:02X}")).collect();
    Ok((tpdu_len, hex))
}

/// Merge concatenated SMS parts when all parts are present, or return standalone ones.
pub fn merge_concat_parts(
    parts: Vec<(u32, DecodedPdu)>,
) -> Vec<(Vec<u32>, DecodedPdu)> {
    let mut result = Vec::new();
    let mut groups: HashMap<(String, u16, u8), Vec<(u32, DecodedPdu)>> = HashMap::new();

    for (idx, pdu) in parts {
        if let Some(c) = &pdu.concat {
            if c.total > 1 {
                groups
                    .entry((pdu.sender.clone(), c.ref_id, c.total))
                    .or_default()
                    .push((idx, pdu));
                continue;
            }
        }
        result.push((vec![idx], pdu));
    }

    for ((_sender, _ref_id, _total), mut group) in groups {
        group.sort_by_key(|(_, p)| p.concat.as_ref().map(|c| c.seq).unwrap_or(0));
        let indices: Vec<u32> = group.iter().map(|(i, _)| *i).collect();
        let merged_text: String = group.iter().map(|(_, p)| p.text.as_str()).collect();
        let mut first = group.into_iter().next().unwrap().1;
        first.text = merged_text;
        first.concat = None;
        result.push((indices, first));
    }

    result
}
