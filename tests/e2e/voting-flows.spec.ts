import { expect, test, type BrowserContext, type Page } from '@playwright/test';

type Visibility = 'public' | 'anonymous';

const PARTICIPANT_COOKIE = 'lite_vote_participant';

type InstrumentedWindow = Window & {
  __e2eCloseEventSource: () => void;
  __e2eEventSourceCount: number;
};

function uniqueRoom(prefix: string) {
  const suffix = `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
  return {
    question: `${prefix} ${suffix}`,
    firstChoice: `選択肢A ${suffix}`,
    secondChoice: `選択肢B ${suffix}`,
  };
}

async function createRoom(
  page: Page,
  visibility: Visibility,
  room: ReturnType<typeof uniqueRoom>,
) {
  await page.goto('/');
  await page.getByLabel('質問').fill(room.question);
  await page.getByRole('textbox', { name: '選択肢 1', exact: true }).fill(room.firstChoice);
  await page.getByRole('textbox', { name: '選択肢 2', exact: true }).fill(room.secondChoice);
  await page.locator(`#visibility-${visibility}`).check();
  await page.getByRole('button', { name: '投票部屋を作成' }).click();
  await expect(page).toHaveURL(/\/rooms\/[A-Za-z0-9_-]+$/);
  return page.url();
}

async function enterPublicRoom(page: Page, roomUrl: string, displayName: string) {
  await page.goto(roomUrl);
  await page.getByLabel('表示名').fill(displayName);
  await page.getByRole('button', { name: '投票部屋へ入る' }).click();
  await expect(page.locator('#voting-room')).toBeVisible();
}

async function waitForRealtime(page: Page) {
  await expect(page.locator('#realtime-connection-status')).toHaveText(
    'リアルタイム更新に接続しました。',
  );
}

async function instrumentEventSource(context: BrowserContext) {
  await context.addInitScript(() => {
    const NativeEventSource = window.EventSource;
    let currentSource: EventSource | undefined;

    class TrackedEventSource extends NativeEventSource {
      constructor(url: string | URL, eventSourceInitDict?: EventSourceInit) {
        super(url, eventSourceInitDict);
        currentSource = this;
        (window as InstrumentedWindow).__e2eEventSourceCount += 1;
      }
    }

    const instrumentedWindow = window as InstrumentedWindow;
    instrumentedWindow.__e2eEventSourceCount = 0;
    instrumentedWindow.__e2eCloseEventSource = () => currentSource?.close();
    window.EventSource = TrackedEventSource;
  });
}

async function vote(page: Page, choice: string) {
  await page.getByRole('radio', { name: choice }).check();
  await page
    .getByRole('button', { name: /^(投票する|投票先を変更する)$/ })
    .click();
}

function resultFor(page: Page, choice: string) {
  return page.locator('#room-results li').filter({
    has: page.getByText(choice, { exact: true }),
  });
}

async function expectIndependentParticipantCookies(
  first: BrowserContext,
  second: BrowserContext,
) {
  const firstCookie = (await first.cookies()).find(
    ({ name }) => name === PARTICIPANT_COOKIE,
  );
  const secondCookie = (await second.cookies()).find(
    ({ name }) => name === PARTICIPANT_COOKIE,
  );

  expect(firstCookie?.value).toBeTruthy();
  expect(secondCookie?.value).toBeTruthy();
  expect(firstCookie?.value).not.toBe(secondCookie?.value);
}

test('公開部屋で投票、投票先変更、締切が別コンテキストへ同期する', async ({
  browser,
}) => {
  const creatorContext = await browser.newContext();
  const participantContext = await browser.newContext();

  try {
    const creator = await creatorContext.newPage();
    const participant = await participantContext.newPage();
    const room = uniqueRoom('公開部屋E2E');
    const roomUrl = await createRoom(creator, 'public', room);

    await enterPublicRoom(creator, roomUrl, 'ありす');
    await enterPublicRoom(participant, roomUrl, 'ぼぶ');
    await expectIndependentParticipantCookies(
      creatorContext,
      participantContext,
    );
    await Promise.all([waitForRealtime(creator), waitForRealtime(participant)]);

    let participantNavigations = 0;
    participant.on('framenavigated', (frame) => {
      if (frame === participant.mainFrame()) participantNavigations += 1;
    });

    await vote(creator, room.firstChoice);
    await expect(resultFor(participant, room.firstChoice)).toContainText(
      '1票（100.0%）',
    );
    await expect(resultFor(participant, room.firstChoice)).toContainText(
      '投票者: ありす',
    );

    await vote(creator, room.secondChoice);
    await expect(resultFor(participant, room.firstChoice)).toContainText('0票（0.0%）');
    await expect(resultFor(participant, room.secondChoice)).toContainText(
      '1票（100.0%）',
    );
    await expect(resultFor(participant, room.secondChoice)).toContainText(
      '投票者: ありす',
    );

    await vote(participant, room.firstChoice);
    await expect(resultFor(creator, room.firstChoice)).toContainText('1票（50.0%）');
    await expect(resultFor(creator, room.firstChoice)).toContainText('投票者: ぼぶ');
    await expect(resultFor(creator, room.secondChoice)).toContainText('1票（50.0%）');

    await creator.getByRole('button', { name: '投票を締め切る' }).click();
    await expect(participant.locator('#room-state')).toHaveText(
      'この投票は締め切られています。',
    );
    await expect(participant.getByRole('heading', { name: '確定結果' })).toBeVisible();
    await expect(participant.locator('form[action$="/votes"]')).toHaveCount(0);
    expect(participantNavigations).toBe(0);
  } finally {
    await Promise.all([creatorContext.close(), participantContext.close()]);
  }
});

test('匿名部屋は名前を表示せず、SSE再接続後に最新結果と締切を同期する', async ({
  browser,
}) => {
  const creatorContext = await browser.newContext();
  const participantContext = await browser.newContext();

  try {
    await instrumentEventSource(participantContext);
    const creator = await creatorContext.newPage();
    const participant = await participantContext.newPage();
    const room = uniqueRoom('匿名部屋E2E');
    const roomUrl = await createRoom(creator, 'anonymous', room);

    await participant.goto(roomUrl);
    await expect(creator.locator('#voting-room')).toBeVisible();
    await expect(participant.locator('#voting-room')).toBeVisible();
    await expect(creator.locator('#room-state')).toHaveText('匿名の投票部屋です。');
    await expect(participant.locator('#room-state')).toHaveText('匿名の投票部屋です。');
    await expectIndependentParticipantCookies(
      creatorContext,
      participantContext,
    );
    await Promise.all([waitForRealtime(creator), waitForRealtime(participant)]);

    let participantNavigations = 0;
    participant.on('framenavigated', (frame) => {
      if (frame === participant.mainFrame()) participantNavigations += 1;
    });

    await vote(creator, room.firstChoice);
    await expect(resultFor(participant, room.firstChoice)).toContainText(
      '1票（100.0%）',
    );
    await expect(participant.locator('#room-results')).not.toContainText('投票者:');

    await participant.evaluate(() => {
      (window as InstrumentedWindow).__e2eCloseEventSource();
    });
    await vote(creator, room.secondChoice);
    await expect(resultFor(creator, room.secondChoice)).toContainText(
      '1票（100.0%）',
    );
    await expect(resultFor(participant, room.firstChoice)).toContainText(
      '1票（100.0%）',
    );

    await participant.evaluate(() => {
      document.dispatchEvent(new Event('visibilitychange'));
    });
    await expect
      .poll(() =>
        participant.evaluate(
          () => (window as InstrumentedWindow).__e2eEventSourceCount,
        ),
      )
      .toBe(2);
    await waitForRealtime(participant);
    await expect(resultFor(participant, room.firstChoice)).toContainText('0票（0.0%）', {
      timeout: 10_000,
    });
    await expect(resultFor(participant, room.secondChoice)).toContainText(
      '1票（100.0%）',
    );
    await expect(participant.locator('#room-results')).not.toContainText('投票者:');

    await vote(participant, room.secondChoice);
    await expect(resultFor(creator, room.secondChoice)).toContainText('2票（100.0%）');

    await creator.getByRole('button', { name: '投票を締め切る' }).click();
    await expect(participant.locator('#room-state')).toHaveText(
      'この投票は締め切られています。',
    );
    await expect(participant.getByRole('heading', { name: '確定結果' })).toBeVisible();
    expect(participantNavigations).toBe(0);
  } finally {
    await Promise.all([creatorContext.close(), participantContext.close()]);
  }
});
