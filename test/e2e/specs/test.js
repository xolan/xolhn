// For authoring Nightwatch tests, see
// http://nightwatchjs.org/guide#usage

module.exports = {
  'Test if header is present and contains text "XolHN"'(browser) {
    browser
    .url('http://localhost:8080')
      .waitForElementVisible('#app', 5000)
      .waitForElementVisible('.vux-header-title', 5000)
      .assert.elementPresent('.vux-header-title')
      .assert.containsText('.vux-header-title', 'XolHN')
      .end();
  },
};
