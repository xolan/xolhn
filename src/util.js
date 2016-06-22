function escapeRegExp(str) {
  return str.replace(/([.*+?^=!:${}()|\[\]\/\\])/g, '\\$1');
}

function replaceAll(str, find, replace) {
  return str.replace(new RegExp(escapeRegExp(find), 'g'), replace);
}

export const checkVisible = (elm) => {
  const rect = elm.getBoundingClientRect();
  const viewHeight = Math.max(document.documentElement.clientHeight, window.innerHeight);
  return !(rect.bottom < 0 || rect.top - viewHeight >= 0);
};

export const fixTextMarkup = (input) => {
  let output = input;
  output = replaceAll(output, '<p>*', '\n*');
  output = replaceAll(output, '<p>', '\n\n');
  output = replaceAll(output, '</p>', '');
  output = replaceAll(output, '<pre><code>', '```\n');
  output = replaceAll(output, '</code></pre>', '```\n');
  return output;
};

export default {
  fixTextMarkup,
  checkVisible,
};
