function escapeRegExp(str) {
  return str.replace(/([.*+?^=!:${}()|\[\]\/\\])/g, '\\$1');
}

function replaceAll(str, find, replace) {
  return str.replace(new RegExp(escapeRegExp(find), 'g'), replace);
}

const fixTextMarkup = (input) => {
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
};
