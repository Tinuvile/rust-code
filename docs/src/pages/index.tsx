import type {ReactNode} from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';

import styles from './index.module.css';

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header className={clsx('hero hero--primary', styles.heroBanner)}>
      <div className="container">
        <Heading as="h1" className="hero__title">
          {siteConfig.title}
        </Heading>
        <p className="hero__subtitle">{siteConfig.tagline}</p>
        <div className={styles.buttons}>
          <Link
            className="button button--secondary button--lg"
            to="/overview/intro">
            开始阅读 →
          </Link>
        </div>
      </div>
    </header>
  );
}

function Highlights() {
  return (
    <section className={styles.highlights}>
      <div className="container">
        <div className="row">
          <div className="col col--4">
            <h3>工业级系统设计</h3>
            <p>
              多云 API 抽象、声明式权限规则、Tree-sitter 安全解析、PII 安全分析、
              渐进式上下文压缩 —— 大规模用户产品的工程实践全景。
            </p>
          </div>
          <div className="col col--4">
            <h3>AI Agent 先行者方案</h3>
            <p>
              子 Agent 分叉/恢复、持久化记忆系统、多 Agent 协调器、技能扩展机制、
              MCP 协议集成 —— Agent 领域先行者的核心设计。
            </p>
          </div>
          <div className="col col--4">
            <h3>按源码目录深入</h3>
            <p>
              从 <code>tools/</code>、<code>services/</code>、<code>utils/</code> 到
              <code>bootstrap/</code>、<code>coordinator/</code>、<code>memdir/</code>
              —— 35+ 个目录逐一拆解。
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}

export default function Home(): ReactNode {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout
      title={siteConfig.title}
      description="Claude Code 架构与设计模式深度解析">
      <HomepageHeader />
      <main>
        <Highlights />
      </main>
    </Layout>
  );
}
