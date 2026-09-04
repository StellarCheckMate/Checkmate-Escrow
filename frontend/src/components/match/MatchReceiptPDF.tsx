/**
 * MatchReceiptPDF — printable payout receipt for completed matches.
 *
 * Renders a PDF document (via @react-pdf/renderer) containing:
 *   - Match ID
 *   - Date of completion
 *   - Player addresses
 *   - Stake amount and token
 *   - Payout amount and recipient
 *   - On-chain transaction hash
 *
 * Usage:
 *   import { downloadMatchReceipt } from './MatchReceiptPDF';
 *   downloadMatchReceipt(props);   // triggers browser download
 */

import {
  Document,
  Page,
  Text,
  View,
  StyleSheet,
  pdf,
} from '@react-pdf/renderer';

export interface MatchReceiptProps {
  matchId: number;
  /** ISO-8601 date string of when the match completed. */
  completedAt: string;
  player1: string;
  player2: string;
  stakeAmount: string;
  token: string;
  /** Amount paid to the winner (same as 2× stake for no-fee matches). */
  payoutAmount: string;
  /** Stellar address of the winner, or "draw" when the match was a draw. */
  winner: string;
  /** On-chain transaction hash of the payout transaction. */
  txHash: string;
}

// ── Styles ────────────────────────────────────────────────────────────────────

const styles = StyleSheet.create({
  page: {
    fontFamily: 'Helvetica',
    fontSize: 11,
    paddingTop: 40,
    paddingBottom: 60,
    paddingHorizontal: 50,
    color: '#1a1a2e',
  },
  header: {
    marginBottom: 24,
    borderBottomWidth: 2,
    borderBottomColor: '#7c3aed',
    paddingBottom: 12,
  },
  title: {
    fontSize: 20,
    fontFamily: 'Helvetica-Bold',
    color: '#7c3aed',
    marginBottom: 4,
  },
  subtitle: {
    fontSize: 11,
    color: '#6b7280',
  },
  section: {
    marginBottom: 16,
  },
  sectionTitle: {
    fontSize: 13,
    fontFamily: 'Helvetica-Bold',
    marginBottom: 8,
    color: '#374151',
    borderBottomWidth: 1,
    borderBottomColor: '#e5e7eb',
    paddingBottom: 4,
  },
  row: {
    flexDirection: 'row',
    marginBottom: 4,
  },
  label: {
    width: 160,
    fontFamily: 'Helvetica-Bold',
    color: '#4b5563',
  },
  value: {
    flex: 1,
    color: '#111827',
    wordBreak: 'break-all',
  },
  footer: {
    position: 'absolute',
    bottom: 30,
    left: 50,
    right: 50,
    fontSize: 9,
    color: '#9ca3af',
    textAlign: 'center',
  },
  badge: {
    backgroundColor: '#7c3aed',
    color: '#ffffff',
    paddingHorizontal: 8,
    paddingVertical: 2,
    borderRadius: 4,
    fontSize: 10,
    alignSelf: 'flex-start',
    marginTop: 2,
  },
});

// ── Document component ────────────────────────────────────────────────────────

/**
 * The PDF document tree rendered by @react-pdf/renderer.
 * Exported for unit-testing the content without triggering a download.
 */
export function MatchReceiptDocument(props: MatchReceiptProps) {
  const {
    matchId,
    completedAt,
    player1,
    player2,
    stakeAmount,
    token,
    payoutAmount,
    winner,
    txHash,
  } = props;

  const formattedDate = new Date(completedAt).toUTCString();

  return (
    <Document
      title={`Match #${matchId} Payout Receipt`}
      author="Checkmate-Escrow"
      subject="Match payout receipt"
    >
      <Page size="A4" style={styles.page}>
        {/* ── Header ──────────────────────────────────────────────────── */}
        <View style={styles.header}>
          <Text style={styles.title}>Checkmate-Escrow</Text>
          <Text style={styles.subtitle}>Match Payout Receipt</Text>
        </View>

        {/* ── Match details ─────────────────────────────────────────── */}
        <View style={styles.section}>
          <Text style={styles.sectionTitle}>Match Details</Text>
          <View style={styles.row}>
            <Text style={styles.label}>Match ID</Text>
            <Text style={styles.value}>#{matchId}</Text>
          </View>
          <View style={styles.row}>
            <Text style={styles.label}>Completed</Text>
            <Text style={styles.value}>{formattedDate}</Text>
          </View>
          <View style={styles.row}>
            <Text style={styles.label}>Player 1</Text>
            <Text style={styles.value}>{player1}</Text>
          </View>
          <View style={styles.row}>
            <Text style={styles.label}>Player 2</Text>
            <Text style={styles.value}>{player2}</Text>
          </View>
        </View>

        {/* ── Payout details ────────────────────────────────────────── */}
        <View style={styles.section}>
          <Text style={styles.sectionTitle}>Payout Details</Text>
          <View style={styles.row}>
            <Text style={styles.label}>Stake Amount</Text>
            <Text style={styles.value}>
              {stakeAmount} {token}
            </Text>
          </View>
          <View style={styles.row}>
            <Text style={styles.label}>Payout Amount</Text>
            <Text style={styles.value}>
              {payoutAmount} {token}
            </Text>
          </View>
          <View style={styles.row}>
            <Text style={styles.label}>Winner</Text>
            <Text style={styles.value}>{winner}</Text>
          </View>
        </View>

        {/* ── Transaction ───────────────────────────────────────────── */}
        <View style={styles.section}>
          <Text style={styles.sectionTitle}>On-Chain Transaction</Text>
          <View style={styles.row}>
            <Text style={styles.label}>Transaction Hash</Text>
            <Text style={styles.value}>{txHash}</Text>
          </View>
        </View>

        {/* ── Footer ────────────────────────────────────────────────── */}
        <Text style={styles.footer}>
          Generated by Checkmate-Escrow · trustless chess wagering on Stellar
          Soroban · match #{matchId}
        </Text>
      </Page>
    </Document>
  );
}

// ── Download helper ───────────────────────────────────────────────────────────

/**
 * Generates the PDF in the browser and triggers a file download.
 *
 * @param props  Receipt data for the completed match.
 */
export async function downloadMatchReceipt(props: MatchReceiptProps): Promise<void> {
  const blob = await pdf(<MatchReceiptDocument {...props} />).toBlob();
  const url = URL.createObjectURL(blob);

  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = `match-receipt-${props.matchId}.pdf`;
  anchor.click();

  // Release the object URL after a short delay to let the download start.
  setTimeout(() => URL.revokeObjectURL(url), 5_000);
}
