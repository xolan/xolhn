<template>
  <div class="item-wrapper"
      v-bind:class="{ isLoading: isLoading, itemWrapperTypeStory: itemData.type === 'story' }">
    <flexbox orient="vertical">
      <flexbox-item>
        <div
            class="item-content item-type-story"
            v-if="itemData.type === 'story' || itemData.type === 'none'">
          <flexbox orient="horizontal" class="item-title-wrapper">
            <flexbox-item class="item-title-main">
              <flexbox orient="vertical">
                <flexbox-item class="item-title">
                  <h1><a :href="itemData.url">{{ itemData.title }}</a></h1>
                </flexbox-item>
                <flexbox-item>
                  <h2>by {{ itemData.by }}</h2>
                </flexbox-item>
              </flexbox>
            </flexbox-item>
            <flexbox-item class="item-title-extra">
              <div>
                <div class="item-title-extra-content">
                  <div class="item-score"><badge v-bind:text="itemData.score.toString()"></badge></div>
                  <div class="item-time">{{ getTimeText }}</div>
                </div>
              </div>
            </flexbox-item>
          </flexbox>
        </div>
        <div
            class="item-content item-type-comment"
            v-bind:style="{ marginLeft: getMargin + 'px', borderLeft: '5px solid ' + getColor }"
            v-if="itemData.type === 'comment' && itemData.deleted !== true">
          <flexbox-item>
            <flexbox orient="horizontal">
              <flexbox-item class="item-title">
                <h3>{{ itemData.by }} wrote:</h3>
              </flexbox-item>
              <flexbox-item class="item-title-extra">
                <div class="item-time">{{ getTimeText }}</div>
              </flexbox-item>
            </flexbox>
          </flexbox-item>
          <div class="item-text">
            <vue-markdown>{{{ getText }}}</vue-markdown>
          </div>
        </div>
      </flexbox-item>
      <flexbox-item v-show="itemData.type === 'story' && getText !== ''">
        <div class="item-story-text">
          <vue-markdown>{{ getText }}</vue-markdown>
        </div>
      </flexbox-item>
      <flexbox-item v-if="debug">
        <div class="item-debug">
          <pre>{{ itemData | json }}</pre>
        </div>
      </flexbox-item>
      <flexbox-item class="item-kids">
        <item-detail
          v-for="kid in itemData.kids"
          :item-key="kid.toString()"
          :level="nextLevel">
        </item-detail>
      </flexbox-item>
    </flexbox>
  </div>
</template>

<script>
import Firebase from 'firebase';
import moment from 'moment';
import Flexbox from 'vux-components/flexbox';
import FlexboxItem from 'vux-components/flexbox-item';
import Spinner from 'vux-components/spinner';
import Badge from 'vux-components/badge';
import VueMarkdown from 'vue-markdown';
import common from '../common';
import util from '../util';

const charMap = {
  block: '█',
};

const initialText = (maxWords, maxWordLength) => {
  const numWords = Math.floor(Math.random() * (maxWords + 1)) || 2;
  const wordLength = Math.floor(Math.random() * (maxWordLength + 1)) || 2;
  return Array(numWords).join(`${Array(wordLength).join(charMap.block)} `);
};

const ItemDetail = {
  name: 'item-detail',
  props: ['itemKey', 'level'],

  data() {
    return {
      // note: changing this line won't causes changes
      // with hot-reload because the reloaded component
      // preserves its current state and we are modifying
      // its initial state.
      debug: false,
      itemData: {
        loading: true,
        type: 'none',
        time: moment().unix(),
        score: 0,
        title: initialText(5, 20),
        text: initialText(50, 20),
        by: initialText(2, 20),
        url: '#',
        kids: [],
      },
    };
  },

  components: {
    Flexbox,
    FlexboxItem,
    Spinner,
    Badge,
    VueMarkdown,
  },

  computed: {
    isLoading() {
      if (this.itemData) {
        if (this.itemData.loading) {
          return this.itemData.loading;
        }
        return false;
      }
      return true;
    },
    getMargin() {
      return (this.getLevel ? (this.getLevel - 1) * 10 : 0);
    },
    getColor() {
      const c = common.vars.colors.levels[this.getLevel.toString()];
      return c;
    },
    getLevel() {
      return this.level || 0;
    },
    getText() {
      let text = this.itemData.text || '';
      text = util.fixTextMarkup(text);
      return text;
    },
    getTimeText() {
      return `${moment.unix(this.itemData.time).fromNow()}`;
    },
    nextLevel() {
      return this.getLevel + 1;
    },
  },

  ready() {
    const itemId = (this.itemKey) ? this.itemKey : this.$route.params.itemId;
    return this.$bindAsObject('itemData', new Firebase(`https://hacker-news.firebaseio.com/v0/item/${itemId}`));
  },
};

export default ItemDetail;
</script>

<!-- Add "scoped" attribute to limit CSS to this component only -->
<style>
  .item-wrapper {
    background: #333;
  }

  .item-wrapper.isLoading * {
    color: #999;
  }

  .item-kids .item-wrapper {
    transition: all 0.5s ease-in-out;
    transform-origin: top;
    transform: scaleY(1);
    opacity: 1;
  }

  .item-kids .item-wrapper.isLoading {
    transform: scaleY(0);
    opacity: 0;
  }

  .item-content {
    padding: 10px;
    padding-left: 0px;
  }

  .item-type-story {
    padding-left: 10px;
    border-bottom: 2px solid #444;
  }

  .item-type-comment {
    margin: 0.5em 0 0 0;
    padding: 10px;
    background: #222;
    color: #ccc;
  }

  .item-text p:not(:last-child) {
    display: block;
    margin-bottom: 1em;
  }

  .item-text pre:not(:last-child) {
    margin-bottom: 1em;
  }

  .item-text a {
    word-break: break-all;
  }

  .item-text ul, .item-story-text ul {
    margin: 0;
    padding: 0;
    padding-bottom: 1em;
  }

  .item-text li, .item-story-text li {
    margin-left: 1.5em;
    padding: 0;
  }

  .item-story-text {
    padding: 10px;
    background: #ccc;
    color: #333;
    margin-bottom: 1em;
  }

  .item-story-text p {
    margin-top: 1em;
  }

  .item-debug {
    padding: 10px;
    background: #ccc;
    color: #333;
    word-break: break-all;
    text-overflow: ellipsis;
    overflow-x: scroll;
  }

  .vux-flexbox-item {
    margin-top: 0px!important;
  }

  .item-title-main {
    align-self: flex-start;
    flex: 5!important;
  }

  .item-title-wrapper * {
    color: #ccc;
  }

  .item-title-wrapper a {
    color: #fff;
  }

  .item-title-extra {
    flex: 1!important;
    align-self: center;
    text-align: right;
  }

  .item-title-extra .vux-badge, .item-title-extra .vux-badge-single {
    padding: 0.75em 1em 0.75em 1em;
    background: #ff6600;
    color: #fff;
    font-size: 1.75em;
  }

</style>
