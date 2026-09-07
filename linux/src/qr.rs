use qrcode::{Color, EcLevel, QrCode};

/// Invitation modules with the four-module quiet zone required by QR scanners.
pub struct InvitationQr {
    pub width: usize,
    pub pixels: Vec<u8>,
}

impl InvitationQr {
    pub fn encode(ticket: &str) -> Result<Self, &'static str> {
        // Match the reference's UTF-8 byte limit before doing encoding work.
        if ticket.is_empty() || ticket.len() > 2953 {
            return Err("linux_qr_unavailable");
        }
        let code = QrCode::with_error_correction_level(ticket.as_bytes(), EcLevel::L)
            .map_err(|_| "linux_qr_unavailable")?;
        let width = code.width() + 8;
        let mut pixels = vec![255; width * width];
        for y in 0..code.width() {
            for x in 0..code.width() {
                if code[(x, y)] == Color::Dark {
                    pixels[(y + 4) * width + x + 4] = 0;
                }
            }
        }
        Ok(Self { width, pixels })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn decode(code: &InvitationQr) -> Vec<u8> {
        let side = code.width * 4;
        let pixels: Vec<_> = (0..side * side)
            .map(|i| code.pixels[(i / side / 4) * code.width + (i % side / 4)])
            .collect();
        let mut decoder = quircs::Quirc::default();
        let codes: Vec<_> = decoder.identify(side, side, &pixels).collect();
        assert_eq!(codes.len(), 1);
        codes
            .into_iter()
            .next()
            .unwrap()
            .unwrap()
            .decode()
            .unwrap()
            .payload
    }

    #[test]
    fn invitation_bytes_round_trip_at_reference_capacity_boundaries() {
        for ticket in [
            "résumé invitation".to_string(),
            "a".repeat(876),
            "a".repeat(2953),
        ] {
            let code = InvitationQr::encode(&ticket).unwrap();
            assert_eq!(decode(&code), ticket.as_bytes());
        }
        assert!(InvitationQr::encode(&"a".repeat(2954)).is_err());
        assert!(InvitationQr::encode(&"é".repeat(1477)).is_err());
        assert!(InvitationQr::encode("").is_err());
    }
}
