import Vue from 'vue';
import VueRouter from 'vue-router';
import VueFire from 'vuefire';

Vue.use(VueRouter);
Vue.use(VueFire);

const app = Vue.extend({});
const router = new VueRouter();

router.map({
  '/': {
    component: (resolve) => {
      require(['./App.vue'], resolve);
    },
    subRoutes: {
      '/': {
        component: (resolve) => {
          require(['./components/TopStories.vue'], resolve);
        },
      },
      '/topstories': {
        component: (resolve) => {
          require(['./components/TopStories.vue'], resolve);
        },
      },
      '/:itemId': {
        name: 'item',
        component: (resolve) => {
          require(['./components/ItemDetail.vue'], resolve);
        },
      },
    },
  },
});

router.start(app, '#app');
