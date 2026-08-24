"use client";

import { useEffect, useState } from "react";
import styles from "./page.module.css";

type PrivacySection = readonly [id: string, label: string];

export function PrivacyToc({ sections }: { sections: readonly PrivacySection[] }) {
  const [activeId, setActiveId] = useState(sections[0]?.[0] ?? "");

  useEffect(() => {
    const sectionElements = sections
      .map(([id]) => document.getElementById(id))
      .filter((section): section is HTMLElement => section !== null);

    if (sectionElements.length === 0) {
      return;
    }

    let frame = 0;

    const updateActiveSection = () => {
      const readingLine = Math.min(window.innerHeight * 0.34, 240);
      let currentId = sectionElements[0].id;

      for (const section of sectionElements) {
        if (section.getBoundingClientRect().top > readingLine) {
          break;
        }

        currentId = section.id;
      }

      const isAtPageEnd =
        window.scrollY + window.innerHeight >= document.documentElement.scrollHeight - 2;

      setActiveId(isAtPageEnd ? sectionElements.at(-1)!.id : currentId);
    };

    const scheduleUpdate = () => {
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(updateActiveSection);
    };

    scheduleUpdate();
    const settleTimer = window.setTimeout(scheduleUpdate, 250);
    window.addEventListener("scroll", scheduleUpdate, { passive: true });
    window.addEventListener("resize", scheduleUpdate);
    window.addEventListener("hashchange", scheduleUpdate);

    return () => {
      window.clearTimeout(settleTimer);
      window.cancelAnimationFrame(frame);
      window.removeEventListener("scroll", scheduleUpdate);
      window.removeEventListener("resize", scheduleUpdate);
      window.removeEventListener("hashchange", scheduleUpdate);
    };
  }, [sections]);

  return (
    <aside className={styles.privacyToc}>
      <p>On this page</p>
      <nav aria-label="Privacy policy sections">
        <ol>
          {sections.map(([id, label]) => (
            <li key={id}>
              <a href={`#${id}`} aria-current={activeId === id ? "location" : undefined}>
                {label}
              </a>
            </li>
          ))}
        </ol>
      </nav>
    </aside>
  );
}
