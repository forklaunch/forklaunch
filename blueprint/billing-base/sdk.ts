import { SchemaValidator } from '@forklaunch/blueprint-core';
import { MapToSdk } from '@forklaunch/core/http';
import {
  cancelSubscription,
  createBillingPortalSession,
  createCheckoutSession,
  createPaymentLink,
  createPlan,
  createSubscription,
  deletePlan,
  deleteSubscription,
  eraseUserData,
  exportUserData,
  expireBillingPortalSession,
  expireCheckoutSession,
  expirePaymentLink,
  getBillingPortalSession,
  getCheckoutSession,
  getOrganizationSubscription,
  getPaymentLink,
  getPlan,
  getSubscription,
  getUserSubscription,
  handleCheckoutFailure,
  handleCheckoutSuccess,
  handlePaymentFailure,
  handlePaymentSuccess,
  listPaymentLinks,
  listPlans,
  listSubscriptions,
  resumeSubscription,
  updateBillingPortalSession,
  updatePaymentLink,
  updatePlan,
  updateSubscription
} from './api/controllers';

export type BillingSdk = {
  compliance: {
    eraseUserData: typeof eraseUserData;
    exportUserData: typeof exportUserData;
  };
  plan: {
    createPlan: typeof createPlan;
    getPlan: typeof getPlan;
    updatePlan: typeof updatePlan;
    deletePlan: typeof deletePlan;
    listPlans: typeof listPlans;
  };
  subscription: {
    createSubscription: typeof createSubscription;
    getSubscription: typeof getSubscription;
    getUserSubscription: typeof getUserSubscription;
    getOrganizationSubscription: typeof getOrganizationSubscription;
    updateSubscription: typeof updateSubscription;
    deleteSubscription: typeof deleteSubscription;
    listSubscriptions: typeof listSubscriptions;
    cancelSubscription: typeof cancelSubscription;
    resumeSubscription: typeof resumeSubscription;
  };
  checkoutSession: {
    createCheckoutSession: typeof createCheckoutSession;
    getCheckoutSession: typeof getCheckoutSession;
    expireCheckoutSession: typeof expireCheckoutSession;
    handleCheckoutSuccess: typeof handleCheckoutSuccess;
    handleCheckoutFailure: typeof handleCheckoutFailure;
  };
  paymentLink: {
    createPaymentLink: typeof createPaymentLink;
    getPaymentLink: typeof getPaymentLink;
    updatePaymentLink: typeof updatePaymentLink;
    expirePaymentLink: typeof expirePaymentLink;
    handlePaymentSuccess: typeof handlePaymentSuccess;
    handlePaymentFailure: typeof handlePaymentFailure;
    listPaymentLinks: typeof listPaymentLinks;
  };
  billingPortal: {
    createBillingPortalSession: typeof createBillingPortalSession;
    getBillingPortalSession: typeof getBillingPortalSession;
    updateBillingPortalSession: typeof updateBillingPortalSession;
    expireBillingPortalSession: typeof expireBillingPortalSession;
  };
};

export const billingSdkClient = {
  compliance: {
    eraseUserData,
    exportUserData
  },
  plan: {
    createPlan: createPlan,
    getPlan: getPlan,
    updatePlan: updatePlan,
    deletePlan: deletePlan,
    listPlans: listPlans
  },
  subscription: {
    createSubscription: createSubscription,
    getSubscription: getSubscription,
    getUserSubscription: getUserSubscription,
    getOrganizationSubscription: getOrganizationSubscription,
    updateSubscription: updateSubscription,
    deleteSubscription: deleteSubscription,
    listSubscriptions: listSubscriptions,
    cancelSubscription: cancelSubscription,
    resumeSubscription: resumeSubscription
  },
  checkoutSession: {
    createCheckoutSession: createCheckoutSession,
    getCheckoutSession: getCheckoutSession,
    expireCheckoutSession: expireCheckoutSession,
    handleCheckoutSuccess: handleCheckoutSuccess,
    handleCheckoutFailure: handleCheckoutFailure
  },
  paymentLink: {
    createPaymentLink: createPaymentLink,
    getPaymentLink: getPaymentLink,
    updatePaymentLink: updatePaymentLink,
    expirePaymentLink: expirePaymentLink,
    handlePaymentSuccess: handlePaymentSuccess,
    handlePaymentFailure: handlePaymentFailure,
    listPaymentLinks: listPaymentLinks
  },
  billingPortal: {
    createBillingPortalSession: createBillingPortalSession,
    getBillingPortalSession: getBillingPortalSession,
    updateBillingPortalSession: updateBillingPortalSession,
    expireBillingPortalSession: expireBillingPortalSession
  }
} satisfies BillingSdk;

export type BillingSdkClient = MapToSdk<SchemaValidator, BillingSdk>;
