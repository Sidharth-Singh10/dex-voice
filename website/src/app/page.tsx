import { SiteNav } from "@/components/SiteNav";
import { Hero } from "@/components/Hero";
import { TruthStrip } from "@/components/TruthStrip";
import { PipelineSection } from "@/components/PipelineSection";
import { VoicesSection } from "@/components/VoicesSection";
import { BehaviorSection } from "@/components/BehaviorSection";
import { EnvelopeSection } from "@/components/EnvelopeSection";
import { StatusSection } from "@/components/StatusSection";
import { SetupSection } from "@/components/SetupSection";
import { FooterCta } from "@/components/FooterCta";

export default function Home() {
  return (
    <>
      <SiteNav />
      <main id="main">
        <Hero />
        <TruthStrip />
        <PipelineSection />
        <VoicesSection />
        <BehaviorSection />
        <EnvelopeSection />
        <StatusSection />
        <SetupSection />
        <FooterCta />
      </main>
    </>
  );
}
